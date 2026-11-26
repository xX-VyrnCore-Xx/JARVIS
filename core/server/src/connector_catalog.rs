//! Catalogo dei connettori pronti (decisione 81, ADR-0047): Gmail, Google Calendar, Outlook, GitHub, Todoist,
//! Slack. I manifest sono incorporati nel binario; l'installazione richiede solo l'ID client OAuth del
//! proprietario (per i servizi OAuth) e passa dalle stesse verifiche di un manifest scritto a mano.

use serde_json::Value;

const INDEX: &str = include_str!("../../../contracts/catalog/connectors/index.json");
const MANIFESTS: &[(&str, &str)] = &[
    (
        "gmail",
        include_str!("../../../contracts/catalog/connectors/gmail.json"),
    ),
    (
        "google_calendar",
        include_str!("../../../contracts/catalog/connectors/google_calendar.json"),
    ),
    (
        "outlook",
        include_str!("../../../contracts/catalog/connectors/outlook.json"),
    ),
    (
        "github",
        include_str!("../../../contracts/catalog/connectors/github.json"),
    ),
    (
        "huggingface",
        include_str!("../../../contracts/catalog/connectors/huggingface.json"),
    ),
    (
        "todoist",
        include_str!("../../../contracts/catalog/connectors/todoist.json"),
    ),
    (
        "slack",
        include_str!("../../../contracts/catalog/connectors/slack.json"),
    ),
    (
        "open_meteo",
        include_str!("../../../contracts/catalog/connectors/open_meteo.json"),
    ),
    (
        "gitlab",
        include_str!("../../../contracts/catalog/connectors/gitlab.json"),
    ),
    (
        "airtable",
        include_str!("../../../contracts/catalog/connectors/airtable.json"),
    ),
];

/// Elenco dei modelli con istruzioni di configurazione.
pub fn index() -> Value {
    serde_json::from_str(INDEX).unwrap_or(Value::Null)
}

/// Manifest pronto per l'installazione: per OAuth inserisce l'ID client del proprietario.
pub fn manifest(id: &str, client_id: Option<&str>) -> Result<Value, String> {
    let raw = MANIFESTS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, m)| *m)
        .ok_or_else(|| format!("connettore non presente nel catalogo: {id}"))?;
    let mut m: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    if m["auth"]["type"] == "oauth2" {
        let cid = client_id
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .ok_or_else(|| {
                "serve l'ID client OAuth creato nella console del servizio (vedi le istruzioni)"
                    .to_string()
            })?;
        if cid.len() > 500 || cid.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("ID client non valido".into());
        }
        m["auth"]["client_id"] = cid.into();
    }
    Ok(m)
}

/// Interpreta il JSON scaricato da Google Cloud per un client OAuth di tipo Desktop.
pub fn google_credentials(file: &Value) -> Result<(&str, &str), String> {
    let installed = &file["installed"];
    let id = installed["client_id"]
        .as_str()
        .filter(|id| {
            id.len() <= 500
                && id
                    .strip_suffix(".apps.googleusercontent.com")
                    .is_some_and(|prefix| {
                        !prefix.is_empty()
                            && prefix
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    })
        })
        .ok_or("Seleziona il JSON di un client OAuth Google di tipo App desktop.")?;
    let secret = installed["client_secret"]
        .as_str()
        .filter(|secret| {
            !secret.is_empty()
                && secret.len() <= 4096
                && secret.bytes().all(|b| b.is_ascii_graphic())
        })
        .ok_or("Il client secret di Google è mancante o non valido.")?;
    Ok((id, secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_a_valid_manifest() {
        let ids: Vec<String> = index()["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids.len(), MANIFESTS.len());
        for id in ids {
            let m = manifest(&id, Some("123-abc.apps.googleusercontent.com")).unwrap();
            let report = jarvis_contracts::validate(jarvis_contracts::Contract::Connector, &m);
            assert!(report.is_valid(), "{id}: {report:?}");
        }
        assert!(manifest("gmail", None).is_err(), "OAuth senza ID client");
        assert!(manifest("gmail", Some("a b")).is_err());
        assert!(manifest("inesistente", None).is_err());
        assert!(manifest("github", None).is_ok(), "token: niente ID client");
    }
}
