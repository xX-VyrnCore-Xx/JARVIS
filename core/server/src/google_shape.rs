//! Gmail e Google Calendar resi facili da usare per il modello (ADR-0239).
//!
//! Le API grezze sono scomode: Gmail elenca solo identificatori (poi serve una lettura per messaggio, con
//! intestazioni in base64 e `payload` annidati), Calendar chiede date RFC 3339 e, senza ordine, parte dal
//! passato. Qui si fissano valori predefiniti sensati prima della richiesta e si riducono le risposte a pochi
//! campi leggibili. Nessuna capacità nuova: stesse operazioni, stessi permessi e conferme.

use base64::Engine;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone};
use serde_json::{json, Map, Value};

/// Intestazioni utili per l'elenco dei messaggi.
pub const MAIL_HEADERS: [&str; 5] = ["From", "To", "Subject", "Date", "Cc"];

fn local_rfc3339(naive: NaiveDateTime) -> String {
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(Local::now)
        .to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Porta una data scritta come «2026-11-28», «2026-11-28T09:00» o già completa a RFC 3339 con il fuso locale.
pub fn to_rfc3339(text: &str, end_of_day: bool) -> Option<String> {
    let t = text.trim();
    if DateTime::parse_from_rfc3339(t).is_ok() {
        return Some(t.to_string());
    }
    if let Ok(day) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        let time = if end_of_day { (23, 59, 59) } else { (0, 0, 0) };
        return Some(local_rfc3339(day.and_hms_opt(time.0, time.1, time.2)?));
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(t, format) {
            return Some(local_rfc3339(naive));
        }
    }
    None
}

/// Parametri di «eventi»: dal momento attuale, in ordine di inizio, eventi singoli (anche le ripetizioni),
/// con `days` come scorciatoia per la finestra («oggi» = 1, «questa settimana» = 7).
pub fn prepare_events(args: &mut Map<String, Value>) {
    let now = Local::now();
    let from = args
        .get("timeMin")
        .and_then(Value::as_str)
        .and_then(|t| to_rfc3339(t, false))
        .unwrap_or_else(|| {
            // Senza data: dall'inizio di oggi, così «oggi» include anche gli eventi già iniziati.
            local_rfc3339(
                now.date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap_or(now.naive_local()),
            )
        });
    let days = args
        .remove("days")
        .and_then(|d| d.as_i64())
        .filter(|d| (1..=90).contains(d));
    if let Some(max) = args
        .get("timeMax")
        .and_then(Value::as_str)
        .and_then(|t| to_rfc3339(t, true))
    {
        args.insert("timeMax".into(), json!(max));
    } else if let Some(days) = days {
        let start = DateTime::parse_from_rfc3339(&from)
            .map(|d| d.with_timezone(&Local))
            .unwrap_or(now);
        let end = start + Duration::days(days);
        args.insert(
            "timeMax".into(),
            json!(end.to_rfc3339_opts(SecondsFormat::Secs, false)),
        );
    } else {
        args.remove("timeMax");
    }
    args.insert("timeMin".into(), json!(from));
    args.entry("singleEvents").or_insert(json!(true));
    if args.get("singleEvents") == Some(&json!(true)) {
        args.entry("orderBy").or_insert(json!("startTime"));
    }
    args.entry("maxResults").or_insert(json!(15));
}

/// Un evento senza fuso orario o senza scostamento è scritto nel fuso locale del computer.
pub fn prepare_new_event(args: &mut Map<String, Value>) {
    for side in ["start", "end"] {
        let Some(obj) = args.get_mut(side).and_then(Value::as_object_mut) else {
            continue;
        };
        if obj.contains_key("date") {
            continue;
        }
        let Some(text) = obj
            .get("dateTime")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let has_zone = obj
            .get("timeZone")
            .is_some_and(|z| z.as_str().is_some_and(|z| !z.is_empty()));
        if DateTime::parse_from_rfc3339(&text).is_ok() {
            continue;
        }
        if has_zone {
            // Con timeZone esplicito, Google vuole l'ora senza scostamento: «2026-11-28T15:00:00».
            let naive = NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M")
                .or_else(|_| NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S"));
            if let Ok(naive) = naive {
                obj.insert(
                    "dateTime".into(),
                    json!(naive.format("%Y-%m-%dT%H:%M:%S").to_string()),
                );
            }
        } else if let Some(full) = to_rfc3339(&text, false) {
            obj.insert("dateTime".into(), json!(full));
        }
    }
    // Senza fine: un'ora dopo l'inizio.
    if args.get("end").is_none() {
        if let Some(start) = args.get("start").cloned() {
            if let Some(text) = start["dateTime"].as_str() {
                if let Ok(begin) = DateTime::parse_from_rfc3339(text) {
                    let end = begin + Duration::hours(1);
                    args.insert(
                        "end".into(),
                        json!({"dateTime": end.to_rfc3339_opts(SecondsFormat::Secs, false)}),
                    );
                }
            }
        }
    }
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// Evento ridotto ai campi che servono per rispondere.
pub fn summarize_event(item: &Value) -> Value {
    let when = |side: &Value| text(&side["dateTime"]).or_else(|| text(&side["date"]));
    let attendees = item["attendees"].as_array().map(|a| {
        a.iter()
            .filter_map(|p| text(&p["displayName"]).or_else(|| text(&p["email"])))
            .take(10)
            .collect::<Vec<_>>()
    });
    let mut out = json!({
        "id": item["id"],
        "titolo": text(&item["summary"]).unwrap_or_else(|| "(senza titolo)".into()),
        "inizio": when(&item["start"]),
        "fine": when(&item["end"]),
        "tutto_il_giorno": item["start"].get("date").is_some(),
    });
    for (key, field) in [
        ("luogo", "location"),
        ("descrizione", "description"),
        ("link", "htmlLink"),
        ("stato", "status"),
    ] {
        if let Some(v) = text(&item[field]) {
            out[key] = json!(v.chars().take(300).collect::<String>());
        }
    }
    if let Some(a) = attendees.filter(|a| !a.is_empty()) {
        out["partecipanti"] = json!(a);
    }
    out
}

pub fn summarize_events(data: &Value) -> Value {
    let items: Vec<Value> = data["items"]
        .as_array()
        .map(|a| a.iter().map(summarize_event).collect())
        .unwrap_or_default();
    json!({
        "calendario": data["summary"], "fuso": data["timeZone"],
        "totale": items.len(), "eventi": items,
        "nota": if items.is_empty() { "Nessun evento nel periodo richiesto." } else { "" },
    })
}

fn header(message: &Value, name: &str) -> Option<String> {
    message["payload"]["headers"]
        .as_array()?
        .iter()
        .find(|h| {
            h["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| text(&h["value"]))
}

/// Testo del messaggio: parte `text/plain` (o `text/html` senza tag), decodificata, al massimo 8000 caratteri.
pub fn message_body(part: &Value) -> Option<String> {
    fn find<'a>(part: &'a Value, mime: &str) -> Option<&'a Value> {
        if part["mimeType"] == mime && part["body"]["data"].is_string() {
            return Some(part);
        }
        part["parts"].as_array()?.iter().find_map(|p| find(p, mime))
    }
    let decode = |part: &Value| -> Option<String> {
        let raw = part["body"]["data"].as_str()?.trim_end_matches('=');
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw)
            .ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    let plain = find(part, "text/plain").and_then(decode);
    let body = plain.or_else(|| {
        find(part, "text/html").and_then(decode).map(|html| {
            let mut out = String::new();
            let mut tag = false;
            for c in html.chars() {
                match c {
                    '<' => tag = true,
                    '>' => {
                        tag = false;
                        out.push(' ');
                    }
                    c if !tag => out.push(c),
                    _ => {}
                }
            }
            out
        })
    })?;
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(compact.chars().take(8000).collect())
}

pub fn summarize_message(message: &Value, with_body: bool) -> Value {
    let mut out = json!({
        "id": message["id"], "conversazione": message["threadId"],
        "da": header(message, "From"), "a": header(message, "To"),
        "oggetto": header(message, "Subject").unwrap_or_else(|| "(senza oggetto)".into()),
        "data": header(message, "Date"),
        "anteprima": text(&message["snippet"]),
        "non_letta": message["labelIds"].as_array().is_some_and(|l| l.iter().any(|x| x == "UNREAD")),
    });
    if let Some(cc) = header(message, "Cc") {
        out["cc"] = json!(cc);
    }
    if with_body {
        if let Some(body) = message_body(&message["payload"]) {
            out["testo"] = json!(body);
        }
    }
    out
}

pub fn summarize_search(list: &Value, messages: Vec<Value>) -> Value {
    json!({
        "totale_stimato": list["resultSizeEstimate"], "mostrati": messages.len(),
        "messaggi": messages,
        "nota": if messages.is_empty() { "Nessun messaggio corrisponde alla ricerca." } else { "" },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn dates_are_completed_with_the_local_zone() {
        assert!(to_rfc3339("2026-11-28", false)
            .unwrap()
            .starts_with("2026-11-28T00:00:00"));
        assert!(to_rfc3339("2026-11-28", true)
            .unwrap()
            .starts_with("2026-11-28T23:59:59"));
        assert!(to_rfc3339("2026-11-28T09:30", false)
            .unwrap()
            .starts_with("2026-11-28T09:30:00"));
        assert_eq!(
            to_rfc3339("2026-11-28T09:30:00Z", false).as_deref(),
            Some("2026-11-28T09:30:00Z")
        );
        assert!(to_rfc3339("domani", false).is_none());
    }

    #[test]
    fn events_default_to_today_onwards_in_order_with_a_day_window() {
        let mut args = map(json!({"days": 7}));
        prepare_events(&mut args);
        assert_eq!(args["singleEvents"], true);
        assert_eq!(args["orderBy"], "startTime");
        assert_eq!(args["maxResults"], 15);
        assert!(args.get("days").is_none());
        let from = DateTime::parse_from_rfc3339(args["timeMin"].as_str().unwrap()).unwrap();
        let to = DateTime::parse_from_rfc3339(args["timeMax"].as_str().unwrap()).unwrap();
        assert!(to - from >= Duration::days(6) && to - from <= Duration::days(8));
        let mut dated =
            map(json!({"timeMin": "2026-12-01", "timeMax": "2026-12-02", "maxResults": 3}));
        prepare_events(&mut dated);
        assert!(dated["timeMin"]
            .as_str()
            .unwrap()
            .starts_with("2026-12-01T00:00:00"));
        assert!(dated["timeMax"]
            .as_str()
            .unwrap()
            .starts_with("2026-12-02T23:59:59"));
        assert_eq!(dated["maxResults"], 3);
    }

    #[test]
    fn new_events_get_a_zone_and_a_default_end() {
        let mut args = map(json!({"summary": "x", "start": {"dateTime": "2026-12-01T15:00"}}));
        prepare_new_event(&mut args);
        assert!(DateTime::parse_from_rfc3339(args["start"]["dateTime"].as_str().unwrap()).is_ok());
        let end = DateTime::parse_from_rfc3339(args["end"]["dateTime"].as_str().unwrap()).unwrap();
        let start =
            DateTime::parse_from_rfc3339(args["start"]["dateTime"].as_str().unwrap()).unwrap();
        assert_eq!(end - start, Duration::hours(1));
        let mut zoned = map(
            json!({"start": {"dateTime": "2026-12-01T15:00", "timeZone": "Europe/Rome"}, "end": {"dateTime": "2026-12-01T16:00", "timeZone": "Europe/Rome"}}),
        );
        prepare_new_event(&mut zoned);
        assert_eq!(zoned["start"]["dateTime"], "2026-12-01T15:00:00");
        let mut all_day =
            map(json!({"start": {"date": "2026-12-01"}, "end": {"date": "2026-12-02"}}));
        prepare_new_event(&mut all_day);
        assert_eq!(all_day["start"]["date"], "2026-12-01");
    }

    #[test]
    fn events_are_reduced_to_readable_fields() {
        let data = json!({"summary": "Lavoro", "timeZone": "Europe/Rome", "items": [
            {"id": "e1", "summary": "Riunione", "start": {"dateTime": "2026-12-01T10:00:00+01:00"},
             "end": {"dateTime": "2026-12-01T11:00:00+01:00"}, "location": "Sala 2",
             "attendees": [{"email": "a@example.test"}, {"displayName": "Bea"}], "etag": "x", "creator": {}},
            {"id": "e2", "start": {"date": "2026-12-02"}, "end": {"date": "2026-12-03"}}]});
        let out = summarize_events(&data);
        assert_eq!(out["totale"], 2);
        assert_eq!(out["eventi"][0]["titolo"], "Riunione");
        assert_eq!(out["eventi"][0]["luogo"], "Sala 2");
        assert_eq!(
            out["eventi"][0]["partecipanti"],
            json!(["a@example.test", "Bea"])
        );
        assert!(out["eventi"][0].get("etag").is_none());
        assert_eq!(out["eventi"][1]["titolo"], "(senza titolo)");
        assert_eq!(out["eventi"][1]["tutto_il_giorno"], true);
        assert!(summarize_events(&json!({"items": []}))["nota"]
            .as_str()
            .unwrap()
            .contains("Nessun evento"));
    }

    #[test]
    fn messages_are_reduced_and_the_body_is_decoded() {
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode("Ciao,\n\nci vediamo   domani.");
        let html =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("<p>Solo <b>html</b></p>");
        let message = json!({"id": "m1", "threadId": "t1", "snippet": "Ciao, ci vediamo", "labelIds": ["UNREAD", "INBOX"],
            "payload": {"mimeType": "multipart/alternative",
              "headers": [{"name": "from", "value": "Anna <anna@example.test>"}, {"name": "Subject", "value": "Domani"}, {"name": "Date", "value": "Mon, 30 Nov 2026 09:00:00 +0100"}],
              "parts": [{"mimeType": "text/plain", "body": {"data": body}}]}});
        let out = summarize_message(&message, true);
        assert_eq!(out["da"], "Anna <anna@example.test>");
        assert_eq!(out["oggetto"], "Domani");
        assert_eq!(out["non_letta"], true);
        assert_eq!(out["testo"], "Ciao, ci vediamo domani.");
        assert!(summarize_message(&message, false).get("testo").is_none());
        let only_html = json!({"mimeType": "text/html", "body": {"data": html}});
        assert_eq!(message_body(&only_html).unwrap(), "Solo html");
        let search = summarize_search(&json!({"resultSizeEstimate": 0}), vec![]);
        assert!(search["nota"]
            .as_str()
            .unwrap()
            .contains("Nessun messaggio"));
    }
}
