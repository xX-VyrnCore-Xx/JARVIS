//! TLS reciproco fra Core e nodi esecutori (RF-11, RNF-04, ADR-0041).
//!
//! Nessuna autorità di certificazione: Core e nodo generano ciascuno una chiave e un certificato
//! autofirmato. All'associazione il nodo invia il proprio certificato (la chiave privata non lascia il
//! nodo) e riceve l'impronta SHA-256 del certificato del Core. Da quel momento:
//! - il Core accetta sulla porta dei nodi solo i certificati client delle impronte registrate e non revocate;
//! - il nodo accetta solo il certificato del Core con l'impronta fissata, indipendentemente dal nome host.
//!
//! rustls verifica comunque la firma dell'handshake con la chiave del certificato presentato: chi ha solo
//! il certificato (pubblico) o solo il token non può fingersi il nodo.

use std::path::Path;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use sha2::Digest;

/// Impronta SHA-256 (esadecimale minuscolo) di un certificato DER.
pub fn fingerprint(der: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(der))
}

/// Certificato DER da PEM; errore leggibile se il testo non è un certificato.
pub fn cert_from_pem(pem: &str) -> Result<CertificateDer<'static>, String> {
    CertificateDer::from_pem_slice(pem.as_bytes())
        .map_err(|e| format!("certificato non valido: {e:?}"))
}

/// Identità TLS: certificato e chiave in PEM.
pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
}

impl Identity {
    /// Nuova chiave ECDSA P-256 con certificato autofirmato per `name`.
    pub fn generate(name: &str) -> Result<Self, String> {
        let ck = rcgen::generate_simple_self_signed(vec![name.to_string()])
            .map_err(|e| e.to_string())?;
        Ok(Self {
            cert_pem: ck.cert.pem(),
            key_pem: ck.signing_key.serialize_pem(),
        })
    }

    pub fn cert(&self) -> Result<CertificateDer<'static>, String> {
        cert_from_pem(&self.cert_pem)
    }

    pub fn key(&self) -> Result<PrivateKeyDer<'static>, String> {
        PrivateKeyDer::from_pem_slice(self.key_pem.as_bytes())
            .map_err(|e| format!("chiave non valida: {e:?}"))
    }

    pub fn fingerprint(&self) -> Result<String, String> {
        Ok(fingerprint(self.cert()?.as_ref()))
    }

    /// Identità del Core in `<dir>/core.crt` e `<dir>/core.key` (0600); creata al primo uso.
    pub fn load_or_create(dir: &Path, name: &str) -> Result<Self, String> {
        let (crt, key) = (dir.join("core.crt"), dir.join("core.key"));
        if let (Ok(cert_pem), Ok(key_pem)) =
            (std::fs::read_to_string(&crt), std::fs::read_to_string(&key))
        {
            return Ok(Self { cert_pem, key_pem });
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let id = Self::generate(name)?;
        std::fs::write(&crt, &id.cert_pem).map_err(|e| e.to_string())?;
        std::fs::write(&key, &id.key_pem).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600));
        }
        Ok(id)
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn schemes() -> Vec<SignatureScheme> {
    provider()
        .signature_verification_algorithms
        .supported_schemes()
}

fn tls12(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
        message,
        cert,
        dss,
        &provider().signature_verification_algorithms,
    )
}

fn tls13(
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
) -> Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
        message,
        cert,
        dss,
        &provider().signature_verification_algorithms,
    )
}

/// Decide se un'impronta client è ammessa (nodo associato e non revocato).
pub type Allow = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Verificatore dei client: certificato obbligatorio, ammesso solo se l'impronta è registrata.
pub struct PinnedClients(pub Allow);

impl std::fmt::Debug for PinnedClients {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PinnedClients")
    }
}

impl ClientCertVerifier for PinnedClients {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        if (self.0)(&fingerprint(end_entity.as_ref())) {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "certificato del nodo non associato o revocato".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        tls12(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        tls13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}

/// Verificatore del server per il nodo: solo il certificato del Core con l'impronta fissata.
#[derive(Debug)]
pub struct PinnedServer(pub String);

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity.as_ref()) == self.0 {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "il Core ha presentato un certificato diverso da quello associato".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        tls12(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        tls13(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        schemes()
    }
}

/// Configurazione del server TLS dei nodi.
pub fn server_config(core: &Identity, allow: Allow) -> Result<rustls::ServerConfig, String> {
    rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_client_cert_verifier(Arc::new(PinnedClients(allow)))
        .with_single_cert(vec![core.cert()?], core.key()?)
        .map_err(|e| e.to_string())
}

/// Configurazione del client TLS del nodo: certificato proprio e Core fissato per impronta.
pub fn client_config(
    node: &Identity,
    core_fingerprint: &str,
) -> Result<rustls::ClientConfig, String> {
    rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedServer(core_fingerprint.to_string())))
        .with_client_auth_cert(vec![node.cert()?], node.key()?)
        .map_err(|e| e.to_string())
}

/// Impronta del certificato client di una connessione TLS (`None` in chiaro).
#[derive(Clone, Debug)]
pub struct PeerCert(pub Option<String>);

/// Listener TLS per `axum::serve`: completa l'handshake prima di passare la connessione; gli handshake
/// falliti (certificato assente, sconosciuto o revocato) vengono scartati senza fermare il server.
pub struct TlsListener {
    pub tcp: tokio::net::TcpListener,
    pub acceptor: tokio_rustls::TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let Ok((tcp, addr)) = self.tcp.accept().await else {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            };
            let handshake = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                self.acceptor.accept(tcp),
            )
            .await;
            if let Ok(Ok(stream)) = handshake {
                return (stream, addr);
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, TlsListener>>
    for PeerCert
{
    fn connect_info(stream: axum::serve::IncomingStream<'_, TlsListener>) -> Self {
        PeerCert(
            stream
                .io()
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|c| c.first())
                .map(|c| fingerprint(c.as_ref())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_have_stable_fingerprints_and_valid_configs() {
        let core = Identity::generate("jarvis-core").unwrap();
        let node = Identity::generate("jarvis-node").unwrap();
        let fp = core.fingerprint().unwrap();
        assert_eq!(fp.len(), 64);
        assert_eq!(
            fp,
            fingerprint(cert_from_pem(&core.cert_pem).unwrap().as_ref())
        );
        assert_ne!(fp, node.fingerprint().unwrap());
        let node_fp = node.fingerprint().unwrap();
        server_config(&core, Arc::new(move |f| f == node_fp)).unwrap();
        client_config(&node, &fp).unwrap();
        assert!(cert_from_pem("non è un certificato").is_err());
    }

    #[test]
    fn pinned_verifiers_accept_only_the_registered_fingerprints() {
        let a = Identity::generate("a").unwrap();
        let b = Identity::generate("b").unwrap();
        let fa = a.fingerprint().unwrap();
        let clients = PinnedClients(Arc::new(move |f| f == fa));
        assert!(clients
            .verify_client_cert(&a.cert().unwrap(), &[], UnixTime::now())
            .is_ok());
        assert!(clients
            .verify_client_cert(&b.cert().unwrap(), &[], UnixTime::now())
            .is_err());
        let server = PinnedServer(b.fingerprint().unwrap());
        let name = ServerName::try_from("qualsiasi.host").unwrap();
        assert!(server
            .verify_server_cert(&b.cert().unwrap(), &[], &name, &[], UnixTime::now())
            .is_ok());
        assert!(server
            .verify_server_cert(&a.cert().unwrap(), &[], &name, &[], UnixTime::now())
            .is_err());
    }
}
