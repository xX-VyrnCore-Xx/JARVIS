//! Mappa e orientamento (ADR-0202): ricerca di luoghi con OpenStreetMap (Nominatim) e percorsi a piedi, in bici o in
//! auto (OSRM del progetto FOSSGIS). Sola lettura, senza chiavi né costi. Fuori dal computer vanno solo i nomi dei
//! luoghi e le coordinate cercati: niente posizione dell'utente, che JARVIS non conosce (nessun GPS).
//!
//! Il servizio pubblico di Nominatim ammette al massimo una richiesta al secondo e richiede un identificativo
//! dell'applicazione (già impostato dal client Web): qui le richieste sono distanziate e mai in blocco.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use url::Url;

use crate::tools::ToolError;
use crate::web::WebConfig;

const NOMINATIM: &str = "https://nominatim.openstreetmap.org";
const ROUTING: &str = "https://routing.openstreetmap.de";

fn err(code: &'static str, message: impl Into<String>) -> ToolError {
    ToolError {
        code,
        message: message.into(),
    }
}

fn base(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Distanzia le richieste a Nominatim (una al secondo, come richiede la sua politica d'uso).
fn pace() {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *last {
        let wait = Duration::from_millis(1100).saturating_sub(at.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
}

fn osm_link(lat: f64, lon: f64) -> String {
    format!("https://www.openstreetmap.org/?mlat={lat:.5}&mlon={lon:.5}#map=16/{lat:.5}/{lon:.5}")
}

/// Coordinate scritte come «45.4642, 9.19»: la ricerca del nome si salta.
fn coordinates(text: &str) -> Option<(f64, f64)> {
    let (a, b) = text.split_once(',')?;
    let (lat, lon) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

fn places_at(
    cfg: &WebConfig,
    base: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<Value>, ToolError> {
    if let Some((lat, lon)) = coordinates(query) {
        return Ok(vec![
            json!({"name": format!("{lat:.5}, {lon:.5}"), "display_name": format!("{lat:.5}, {lon:.5}"),
            "lat": lat, "lon": lon, "category": "coordinate", "map_url": osm_link(lat, lon)}),
        ]);
    }
    let mut url = Url::parse(&format!("{base}/search"))
        .map_err(|_| err("map.invalid", "servizio di mappe non valido"))?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "jsonv2")
        .append_pair("limit", &limit.to_string())
        .append_pair("accept-language", "it");
    pace();
    let page = crate::web::get_trusted(cfg, url)?;
    let data: Value = serde_json::from_slice(&page).map_err(|_| {
        err(
            "map.invalid_response",
            "risposta del servizio di mappe non valida",
        )
    })?;
    Ok(data
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let lat: f64 = p["lat"].as_str()?.parse().ok()?;
            let lon: f64 = p["lon"].as_str()?.parse().ok()?;
            let display = p["display_name"].as_str()?;
            Some(json!({
                "name": p["name"].as_str().filter(|n| !n.is_empty()).unwrap_or_else(|| display.split(',').next().unwrap_or(display)),
                "display_name": display.chars().take(240).collect::<String>(),
                "lat": lat, "lon": lon,
                "category": p["category"].as_str().unwrap_or(""), "type": p["type"].as_str().unwrap_or(""),
                "map_url": osm_link(lat, lon),
            }))
        })
        .collect())
}

/// `map.search`: luoghi che corrispondono a un nome (o già coordinate), con coordinate e collegamento alla mappa.
pub fn search(cfg: &WebConfig, args: &Value) -> Result<Value, ToolError> {
    search_at(cfg, &base("JARVIS_NOMINATIM_URL", NOMINATIM), args)
}

fn search_at(cfg: &WebConfig, base: &str, args: &Value) -> Result<Value, ToolError> {
    let query = args["query"]
        .as_str()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| err("map.invalid", "Indica il luogo da cercare"))?;
    let limit = args["limit"].as_u64().unwrap_or(5).clamp(1, 10) as usize;
    let places = places_at(cfg, base, query, limit)?;
    Ok(
        json!({"query": query, "places": places, "source": "OpenStreetMap (Nominatim)",
        "retrieved_at": crate::now(), "untrusted": true,
        "note": "Il nome cercato è uscito dal computer; la tua posizione no."}),
    )
}

/// Frase italiana per un passo di OSRM (tipo di manovra, direzione, nome della via).
pub fn instruction(maneuver: &Value, name: &str) -> String {
    let kind = maneuver["type"].as_str().unwrap_or("");
    let modifier = maneuver["modifier"].as_str().unwrap_or("");
    let way = match modifier {
        "left" => "a sinistra",
        "right" => "a destra",
        "slight left" => "leggermente a sinistra",
        "slight right" => "leggermente a destra",
        "sharp left" => "stretto a sinistra",
        "sharp right" => "stretto a destra",
        "uturn" => "con inversione a U",
        _ => "dritto",
    };
    let street = if name.is_empty() {
        String::new()
    } else {
        format!(" su {name}")
    };
    match kind {
        "depart" => format!("Parti{street}"),
        "arrive" => "Sei arrivato a destinazione".to_string(),
        "turn" | "end of road" => format!("Gira {way}{street}"),
        "fork" => format!("Al bivio tieni {way}{street}"),
        "merge" => format!("Immettiti {way}{street}"),
        "on ramp" | "off ramp" => format!("Prendi la rampa {way}{street}"),
        "roundabout" | "rotary" | "roundabout turn" => {
            let exit = maneuver["exit"]
                .as_u64()
                .map(|n| format!(" ed esci alla {n}ª uscita"))
                .unwrap_or_default();
            format!("Alla rotatoria{exit}{street}")
        }
        "exit roundabout" | "exit rotary" => format!("Esci dalla rotatoria{street}"),
        "new name" | "continue" => format!("Continua {way}{street}"),
        _ => format!("Prosegui {way}{street}"),
    }
}

/// `map.route`: percorso fra due luoghi (nomi o coordinate) a piedi, in bici o in auto, con distanza, durata e passi.
pub fn route(cfg: &WebConfig, args: &Value) -> Result<Value, ToolError> {
    route_at(
        cfg,
        &base("JARVIS_NOMINATIM_URL", NOMINATIM),
        &base("JARVIS_ROUTING_URL", ROUTING),
        args,
    )
}

fn route_at(cfg: &WebConfig, geo: &str, routing: &str, args: &Value) -> Result<Value, ToolError> {
    let text = |key: &str| {
        args[key]
            .as_str()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                err(
                    "map.invalid",
                    format!("Indica «{key}»: un luogo o le coordinate"),
                )
            })
    };
    let (from_text, to_text) = (text("from")?, text("to")?);
    let (profile, label) = match args["mode"].as_str().unwrap_or("auto") {
        "piedi" | "foot" | "walking" => ("routed-foot", "a piedi"),
        "bici" | "bike" | "cycling" => ("routed-bike", "in bici"),
        "auto" | "car" | "driving" => ("routed-car", "in auto"),
        _ => {
            return Err(err(
                "map.invalid",
                "Modalità non valida: auto, piedi o bici",
            ))
        }
    };
    let from = places_at(cfg, geo, from_text, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| err("map.not_found", format!("Non trovo «{from_text}»")))?;
    let to = places_at(cfg, geo, to_text, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| err("map.not_found", format!("Non trovo «{to_text}»")))?;
    let (a, b) = (
        (
            from["lat"].as_f64().unwrap_or(0.0),
            from["lon"].as_f64().unwrap_or(0.0),
        ),
        (
            to["lat"].as_f64().unwrap_or(0.0),
            to["lon"].as_f64().unwrap_or(0.0),
        ),
    );
    let mut url = Url::parse(&format!(
        "{routing}/{profile}/route/v1/driving/{:.6},{:.6};{:.6},{:.6}",
        a.1, a.0, b.1, b.0
    ))
    .map_err(|_| err("map.invalid", "servizio di percorsi non valido"))?;
    url.query_pairs_mut()
        .append_pair("overview", "simplified")
        .append_pair("geometries", "geojson")
        .append_pair("steps", "true");
    let body = crate::web::get_trusted(cfg, url)?;
    let data: Value = serde_json::from_slice(&body).map_err(|_| {
        err(
            "map.invalid_response",
            "risposta del servizio di percorsi non valida",
        )
    })?;
    if data["code"] != "Ok" {
        return Err(err(
            "map.no_route",
            format!(
                "Nessun percorso {label} fra i due luoghi ({})",
                data["code"].as_str().unwrap_or("errore")
            ),
        ));
    }
    let route = &data["routes"][0];
    let distance = route["distance"].as_f64().unwrap_or(0.0);
    let duration = route["duration"].as_f64().unwrap_or(0.0);
    let steps: Vec<Value> = route["legs"][0]["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .take(60)
        .map(|s| {
            json!({"instruction": instruction(&s["maneuver"], s["name"].as_str().unwrap_or("")),
                "distance_m": s["distance"].as_f64().unwrap_or(0.0).round()})
        })
        .collect();
    // Tracciato per disegnare il percorso sulla mappa: al massimo 300 punti [lat, lon].
    let line: Vec<&Value> = route["geometry"]["coordinates"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let stride = line.len().div_ceil(300).max(1);
    let path: Vec<Value> = line
        .iter()
        .step_by(stride)
        .chain(line.last())
        .filter_map(|p| Some(json!([p[1].as_f64()?, p[0].as_f64()?])))
        .collect();
    let minutes = (duration / 60.0).round() as u64;
    let summary = format!(
        "{label}: {} km, circa {} min.",
        format!("{:.1}", distance / 1000.0).replace('.', ","),
        minutes.max(1)
    );
    let directions = format!(
        "https://www.openstreetmap.org/directions?engine=fossgis_osrm_{}&route={:.5}%2C{:.5}%3B{:.5}%2C{:.5}",
        match profile {
            "routed-foot" => "foot",
            "routed-bike" => "bike",
            _ => "car",
        },
        a.0, a.1, b.0, b.1
    );
    Ok(
        json!({"from": from, "to": to, "mode": label, "distance_m": distance.round(), "duration_s": duration.round(),
        "summary": summary, "steps": steps, "path": path, "map_url": directions, "source": "OpenStreetMap (Nominatim, OSRM)",
        "retrieved_at": crate::now(), "untrusted": true,
        "note": "Percorso indicativo senza traffico in tempo reale: controlla lavori e chiusure prima di partire."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    /// Server di prova: `/search` restituisce un luogo scelto dal nome, `/…/route/…` un percorso fisso.
    fn serve() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                    loop {
                        let mut h = String::new();
                        if reader.read_line(&mut h).unwrap_or(0) <= 2 {
                            break;
                        }
                    }
                    let body = if path.starts_with("/search") {
                        let q = Url::parse(&format!("http://x{path}")).unwrap();
                        let name = q
                            .query_pairs()
                            .find(|(k, _)| k == "q")
                            .map(|(_, v)| v.into_owned())
                            .unwrap_or_default();
                        if name.contains("Nowhere") {
                            "[]".to_string()
                        } else {
                            let (lat, lon) = if name.contains("Milano") {
                                ("45.46427", "9.18951")
                            } else {
                                ("45.07049", "7.68682")
                            };
                            format!(
                                r#"[{{"lat":"{lat}","lon":"{lon}","name":"{name}","display_name":"{name}, Italia","category":"boundary","type":"administrative"}}]"#
                            )
                        }
                    } else if path.contains("/route/v1/") {
                        r#"{"code":"Ok","routes":[{"distance":142300.4,"duration":6300.0,"geometry":{"coordinates":[[7.68682,45.07049],[8.5,45.3],[9.18951,45.46427]]},"legs":[{"steps":[
                            {"maneuver":{"type":"depart","modifier":"straight"},"name":"Via Roma","distance":300.0},
                            {"maneuver":{"type":"turn","modifier":"left"},"name":"Corso Francia","distance":1200.0},
                            {"maneuver":{"type":"roundabout","exit":2},"name":"","distance":80.0},
                            {"maneuver":{"type":"arrive"},"name":"","distance":0.0}]}]}]}"#.to_string()
                    } else {
                        "{}".to_string()
                    };
                    let mut s = stream;
                    let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                });
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    fn local() -> WebConfig {
        WebConfig {
            allow_private: true,
            timeout: Duration::from_secs(3),
            ..WebConfig::default()
        }
    }

    #[test]
    fn places_and_routes_are_read_from_the_map_services_with_italian_directions() {
        let server = serve();
        let found = search_at(&local(), &server, &json!({"query": "Milano"})).unwrap();
        assert_eq!(found["places"][0]["lat"], 45.46427);
        assert!(found["places"][0]["map_url"]
            .as_str()
            .unwrap()
            .starts_with("https://www.openstreetmap.org/"));
        let route = route_at(
            &local(),
            &server,
            &server,
            &json!({"from": "Torino", "to": "Milano", "mode": "auto"}),
        )
        .unwrap();
        assert_eq!(route["summary"], "in auto: 142,3 km, circa 105 min.");
        let steps: Vec<&str> = route["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s["instruction"].as_str())
            .collect();
        assert_eq!(
            steps,
            [
                "Parti su Via Roma",
                "Gira a sinistra su Corso Francia",
                "Alla rotatoria ed esci alla 2ª uscita",
                "Sei arrivato a destinazione"
            ]
        );
        assert!(route["map_url"]
            .as_str()
            .unwrap()
            .contains("fossgis_osrm_car"));
        assert_eq!(route["path"][0], json!([45.07049, 7.68682]));
        assert_eq!(
            route["path"].as_array().unwrap().last().unwrap(),
            &json!([45.46427, 9.18951])
        );
        // Le coordinate scritte a mano non interrogano la ricerca dei nomi.
        let by_coordinates = route_at(
            &local(),
            &server,
            &server,
            &json!({"from": "45.07, 7.69", "to": "Milano", "mode": "piedi"}),
        )
        .unwrap();
        assert!(by_coordinates["summary"]
            .as_str()
            .unwrap()
            .starts_with("a piedi"));
        assert_eq!(by_coordinates["from"]["category"], "coordinate");
    }

    #[test]
    fn unknown_places_modes_and_missing_arguments_are_explained() {
        let server = serve();
        assert_eq!(
            route_at(
                &local(),
                &server,
                &server,
                &json!({"from": "Nowhere", "to": "Milano"})
            )
            .unwrap_err()
            .code,
            "map.not_found"
        );
        assert_eq!(
            route_at(
                &local(),
                &server,
                &server,
                &json!({"from": "Torino", "to": "Milano", "mode": "razzo"})
            )
            .unwrap_err()
            .code,
            "map.invalid"
        );
        assert_eq!(
            route_at(&local(), &server, &server, &json!({"from": "Torino"}))
                .unwrap_err()
                .code,
            "map.invalid"
        );
        assert_eq!(
            search_at(&local(), &server, &json!({})).unwrap_err().code,
            "map.invalid"
        );
        assert_eq!(coordinates("91, 10"), None);
        assert_eq!(coordinates("non coordinate"), None);
    }
}
