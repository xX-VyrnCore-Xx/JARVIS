//! Rete HTTPS in uscita condivisa (ADR-0108): radici TLS, agenti, diagnosi delle cause di errore.
//!
//! `ureq` con la sola funzione `native-certs` si fida soltanto dell'archivio del sistema: se è incompleto
//! (Windows che installa le radici su richiesta, Linux senza `ca-certificates`, archivio illeggibile) ogni
//! connessione fallisce con un errore generico. Qui le radici del sistema sono unite a quelle incluse.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// Perché una richiesta HTTPS non è partita o si è interrotta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Dns,
    Refused,
    Timeout,
    Reset,
    Certificate,
    Proxy,
    Other,
}

impl Cause {
    pub fn code(self) -> &'static str {
        match self {
            Self::Dns => "dns",
            Self::Refused => "refused",
            Self::Timeout => "timeout",
            Self::Reset => "reset",
            Self::Certificate => "certificate",
            Self::Proxy => "proxy",
            Self::Other => "other",
        }
    }

    /// Spiegazione per il proprietario, con la correzione più probabile.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Dns => "il nome del server non si risolve: controlla la connessione Internet e il DNS",
            Self::Refused => "il server non accetta la connessione: controlla firewall, VPN e rete",
            Self::Timeout => "il server non ha risposto in tempo: riprova o scegli un modello più leggero",
            Self::Reset => "la connessione è stata interrotta dalla rete: riprova",
            Self::Certificate => "il certificato del server non è considerato valido: controlla data e ora del computer, antivirus o proxy che ispezionano il traffico",
            Self::Proxy => "il proxy ha rifiutato la connessione: controlla HTTPS_PROXY e le credenziali del proxy",
            Self::Other => "errore di rete non classificato: controlla rete, certificati e proxy",
        }
    }

    /// Cause tipicamente transitorie, per cui un nuovo tentativo ha senso. Un timeout in lettura no:
    /// la richiesta potrebbe essere già stata elaborata.
    pub fn transient(self) -> bool {
        matches!(self, Self::Dns | Self::Refused | Self::Reset)
    }
}

/// Classifica un errore di trasporto a partire dal tipo e dal testo (sorgenti incluse).
pub fn classify_text(kind: &str, text: &str) -> Cause {
    let t = text.to_ascii_lowercase();
    match kind {
        "Dns" => return Cause::Dns,
        "ProxyConnect" | "InvalidProxyUrl" | "ProxyUnauthorized" => return Cause::Proxy,
        _ => {}
    }
    if [
        "certificate",
        "unknownissuer",
        "unknown issuer",
        "invalid peer",
        "notvalidforname",
        "expired",
        "badsignature",
    ]
    .iter()
    .any(|k| t.contains(k))
    {
        return Cause::Certificate;
    }
    if ["timed out", "timeout", "deadline"]
        .iter()
        .any(|k| t.contains(k))
    {
        return Cause::Timeout;
    }
    if ["reset", "broken pipe", "eof", "aborted", "closed"]
        .iter()
        .any(|k| t.contains(k))
    {
        return Cause::Reset;
    }
    if kind == "ConnectionFailed" || t.contains("refused") || t.contains("unreachable") {
        return Cause::Refused;
    }
    Cause::Other
}

pub fn classify(error: &ureq::Transport) -> Cause {
    use std::error::Error;
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(inner) = source {
        text.push(' ');
        text.push_str(&inner.to_string());
        source = inner.source();
    }
    classify_text(&format!("{:?}", error.kind()), &text)
}

fn roots() -> rustls::RootCertStore {
    let mut store = rustls::RootCertStore::empty();
    // Un archivio di sistema illeggibile non deve bloccare la rete: restano le radici incluse.
    if let Ok(native) = rustls_native_certs::load_native_certs() {
        store.add_parsable_certificates(native);
    }
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    store
}

/// Configurazione TLS condivisa: radici del sistema + radici incluse, provider `ring`.
pub fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("versioni TLS")
                .with_root_certificates(roots())
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// Agente base: radici condivise, nessun redirect (le credenziali non vengono inoltrate), connessione con tempo massimo.
pub fn builder(total: Duration) -> ureq::AgentBuilder {
    ureq::AgentBuilder::new()
        .tls_config(tls_config())
        .timeout_connect(Duration::from_secs(10))
        .timeout(total)
        .redirects(0)
        .user_agent(concat!("JARVIS/", env!("CARGO_PKG_VERSION")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causes_are_recognised_from_kind_and_text() {
        assert_eq!(classify_text("Dns", "failed to lookup"), Cause::Dns);
        assert_eq!(classify_text("ProxyConnect", "x"), Cause::Proxy);
        assert_eq!(
            classify_text("Io", "invalid peer certificate: UnknownIssuer"),
            Cause::Certificate
        );
        assert_eq!(classify_text("Io", "operation timed out"), Cause::Timeout);
        assert_eq!(
            classify_text("Io", "connection reset by peer"),
            Cause::Reset
        );
        assert_eq!(classify_text("ConnectionFailed", "x"), Cause::Refused);
        assert_eq!(classify_text("Io", "boh"), Cause::Other);
        assert!(
            Cause::Dns.transient()
                && !Cause::Timeout.transient()
                && !Cause::Certificate.transient()
        );
    }

    #[test]
    fn the_trust_store_always_has_the_bundled_roots() {
        // Anche con l'archivio del sistema vuoto restano le radici incluse.
        assert!(roots().len() >= webpki_roots::TLS_SERVER_ROOTS.len());
        let _ = tls_config();
    }

    #[test]
    fn a_refused_connection_is_classified() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let error = builder(Duration::from_secs(3))
            .build()
            .get(&format!("http://127.0.0.1:{port}/"))
            .call()
            .unwrap_err();
        let ureq::Error::Transport(t) = error else {
            panic!("atteso un errore di trasporto")
        };
        assert_eq!(classify(&t), Cause::Refused);
    }
}
