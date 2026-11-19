//! Passkey (WebAuthn) per i dispositivi associati (RNF-04, ADR-0042).
//!
//! Un dispositivo associato registra una passkey (ES256, verifica dell'utente obbligatoria); da quel momento
//! approvare una conferma da quel dispositivo richiede, oltre al token, un'asserzione firmata dalla passkey e
//! legata a quella conferma. Il Core conserva solo la chiave pubblica. Attestazione «none»: si verifica il
//! possesso della chiave, non il modello dell'autenticatore.

use base64::Engine;
use serde_json::Value;
use sha2::Digest;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

pub fn b64url(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn from_b64url(text: &str) -> Result<Vec<u8>, String> {
    B64.decode(text.trim_end_matches('='))
        .map_err(|_| "base64url non valido".to_string())
}

// ------------------------------------------------------------------ CBOR minimo (RFC 8949)

#[derive(Debug, Clone, PartialEq)]
pub enum Cbor {
    Int(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Cbor>),
    Map(Vec<(Cbor, Cbor)>),
    Other,
}

impl Cbor {
    pub fn get(&self, key: &Cbor) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// Decodifica un elemento CBOR; restituisce il valore e i byte consumati. Solo forme definite.
pub fn cbor(data: &[u8]) -> Result<(Cbor, usize), String> {
    fn arg(data: &[u8], info: u8, pos: &mut usize) -> Result<u64, String> {
        let take = |n: usize, pos: &mut usize| -> Result<u64, String> {
            let b = data.get(*pos..*pos + n).ok_or("CBOR troncato")?;
            *pos += n;
            Ok(b.iter().fold(0u64, |acc, x| (acc << 8) | u64::from(*x)))
        };
        match info {
            0..=23 => Ok(u64::from(info)),
            24 => take(1, pos),
            25 => take(2, pos),
            26 => take(4, pos),
            27 => take(8, pos),
            _ => Err("CBOR a lunghezza indefinita non ammesso".into()),
        }
    }
    fn item(data: &[u8], pos: &mut usize, depth: u8) -> Result<Cbor, String> {
        if depth > 16 {
            return Err("CBOR troppo annidato".into());
        }
        let head = *data.get(*pos).ok_or("CBOR troncato")?;
        *pos += 1;
        let (major, info) = (head >> 5, head & 0x1f);
        let n = arg(data, info, pos)?;
        let len = usize::try_from(n).map_err(|_| "CBOR troppo grande")?;
        Ok(match major {
            0 => Cbor::Int(i64::try_from(n).map_err(|_| "intero CBOR troppo grande")?),
            1 => Cbor::Int(-1 - i64::try_from(n).map_err(|_| "intero CBOR troppo grande")?),
            2 | 3 => {
                let b = data.get(*pos..*pos + len).ok_or("CBOR troncato")?.to_vec();
                *pos += len;
                if major == 2 {
                    Cbor::Bytes(b)
                } else {
                    Cbor::Text(String::from_utf8(b).map_err(|_| "testo CBOR non UTF-8")?)
                }
            }
            4 => {
                let mut v = Vec::with_capacity(len.min(64));
                for _ in 0..len {
                    v.push(item(data, pos, depth + 1)?);
                }
                Cbor::Array(v)
            }
            5 => {
                let mut m = Vec::with_capacity(len.min(64));
                for _ in 0..len {
                    let k = item(data, pos, depth + 1)?;
                    let v = item(data, pos, depth + 1)?;
                    m.push((k, v));
                }
                Cbor::Map(m)
            }
            _ => Cbor::Other,
        })
    }
    let mut pos = 0;
    let v = item(data, &mut pos, 0)?;
    Ok((v, pos))
}

// ------------------------------------------------------------------ verifica WebAuthn

/// Dati dell'autenticatore (WebAuthn §6.1).
pub struct AuthData {
    pub rp_id_hash: [u8; 32],
    pub flags: u8,
    pub sign_count: u32,
    /// Credenziale registrata: id e chiave pubblica non compressa (0x04 || x || y).
    pub credential: Option<(Vec<u8>, Vec<u8>)>,
}

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const AT: u8 = 0x40;

pub fn parse_auth_data(data: &[u8]) -> Result<AuthData, String> {
    if data.len() < 37 {
        return Err("authenticatorData troppo corto".into());
    }
    let mut rp_id_hash = [0u8; 32];
    rp_id_hash.copy_from_slice(&data[..32]);
    let flags = data[32];
    let sign_count = u32::from_be_bytes([data[33], data[34], data[35], data[36]]);
    let credential = if flags & AT != 0 {
        let rest = &data[37..];
        if rest.len() < 18 {
            return Err("dati della credenziale troppo corti".into());
        }
        let id_len = usize::from(u16::from_be_bytes([rest[16], rest[17]]));
        let id = rest
            .get(18..18 + id_len)
            .ok_or("id della credenziale troncato")?
            .to_vec();
        let (key, _) = cbor(&rest[18 + id_len..])?;
        // COSE_Key EC2 P-256 ES256: kty 2, alg -7, crv 1, x -2, y -3.
        let ok = key.get(&Cbor::Int(1)) == Some(&Cbor::Int(2))
            && key.get(&Cbor::Int(3)) == Some(&Cbor::Int(-7))
            && key.get(&Cbor::Int(-1)) == Some(&Cbor::Int(1));
        let (Some(Cbor::Bytes(x)), Some(Cbor::Bytes(y))) =
            (key.get(&Cbor::Int(-2)), key.get(&Cbor::Int(-3)))
        else {
            return Err("chiave COSE senza coordinate".into());
        };
        if !ok || x.len() != 32 || y.len() != 32 {
            return Err("è ammessa solo una chiave ES256 (P-256)".into());
        }
        let mut public = vec![0x04];
        public.extend_from_slice(x);
        public.extend_from_slice(y);
        Some((id, public))
    } else {
        None
    };
    Ok(AuthData {
        rp_id_hash,
        flags,
        sign_count,
        credential,
    })
}

/// Controlli comuni su clientDataJSON e authenticatorData: tipo, sfida, origine, RP, presenza e verifica utente.
fn check_common(
    client_data: &[u8],
    auth: &AuthData,
    kind: &str,
    challenge: &str,
    origins: &[String],
    rp_id: &str,
) -> Result<(), String> {
    let cd: Value = serde_json::from_slice(client_data).map_err(|_| "clientDataJSON non valido")?;
    if cd["type"] != kind {
        return Err(format!(
            "tipo di operazione WebAuthn inatteso (atteso {kind})"
        ));
    }
    if cd["challenge"].as_str() != Some(challenge) {
        return Err("sfida della passkey non corrispondente o scaduta".into());
    }
    let origin = cd["origin"].as_str().unwrap_or_default();
    if !origins.iter().any(|o| o == origin) {
        return Err(format!("origine non ammessa: {origin}"));
    }
    if auth.rp_id_hash[..] != sha2::Sha256::digest(rp_id.as_bytes())[..] {
        return Err("passkey creata per un altro sito".into());
    }
    if auth.flags & UP == 0 || auth.flags & UV == 0 {
        return Err("serve la verifica dell'utente (impronta, volto o PIN)".into());
    }
    Ok(())
}

/// Credenziale registrata: id e chiave pubblica.
pub struct Registered {
    pub credential_id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

/// Verifica una registrazione (`navigator.credentials.create`).
pub fn verify_registration(
    client_data: &[u8],
    attestation_object: &[u8],
    challenge: &str,
    origins: &[String],
    rp_id: &str,
) -> Result<Registered, String> {
    let (att, _) = cbor(attestation_object)?;
    let Some(Cbor::Bytes(auth_data)) = att.get(&Cbor::Text("authData".into())) else {
        return Err("attestationObject senza authData".into());
    };
    let auth = parse_auth_data(auth_data)?;
    check_common(
        client_data,
        &auth,
        "webauthn.create",
        challenge,
        origins,
        rp_id,
    )?;
    let (credential_id, public_key) = auth.credential.ok_or("registrazione senza credenziale")?;
    Ok(Registered {
        credential_id,
        public_key,
        sign_count: auth.sign_count,
    })
}

/// Verifica un'asserzione (`navigator.credentials.get`) con la chiave pubblica registrata; restituisce il
/// nuovo contatore. Un contatore che non cresce (se usato dall'autenticatore) indica una passkey clonata.
#[allow(clippy::too_many_arguments)]
pub fn verify_assertion(
    client_data: &[u8],
    authenticator_data: &[u8],
    signature: &[u8],
    public_key: &[u8],
    stored_count: u32,
    challenge: &str,
    origins: &[String],
    rp_id: &str,
) -> Result<u32, String> {
    let auth = parse_auth_data(authenticator_data)?;
    check_common(
        client_data,
        &auth,
        "webauthn.get",
        challenge,
        origins,
        rp_id,
    )?;
    let mut signed = authenticator_data.to_vec();
    signed.extend_from_slice(&sha2::Sha256::digest(client_data));
    ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_ASN1, public_key)
        .verify(&signed, signature)
        .map_err(|_| "firma della passkey non valida".to_string())?;
    if (auth.sign_count != 0 || stored_count != 0) && auth.sign_count <= stored_count {
        return Err("contatore della passkey non cresciuto: possibile copia".into());
    }
    Ok(auth.sign_count)
}

/// Autenticatore sintetico per i test: genera le stesse strutture di un autenticatore reale.
pub mod testing {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};

    pub struct Authenticator {
        key: EcdsaKeyPair,
        pub credential_id: Vec<u8>,
        pub count: u32,
        rp_id: String,
        origin: String,
    }

    fn cbor_bytes(out: &mut Vec<u8>, major: u8, b: &[u8]) {
        head(out, major, b.len() as u64);
        out.extend_from_slice(b);
    }

    fn head(out: &mut Vec<u8>, major: u8, n: u64) {
        let m = major << 5;
        match n {
            0..=23 => out.push(m | n as u8),
            24..=0xff => out.extend_from_slice(&[m | 24, n as u8]),
            _ => {
                out.push(m | 25);
                out.extend_from_slice(&(n as u16).to_be_bytes());
            }
        }
    }

    fn int(out: &mut Vec<u8>, v: i64) {
        if v >= 0 {
            head(out, 0, v as u64)
        } else {
            head(out, 1, (-1 - v) as u64)
        }
    }

    impl Authenticator {
        pub fn new(rp_id: &str, origin: &str) -> Self {
            let rng = SystemRandom::new();
            let pkcs8 =
                EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
            let key =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                    .unwrap();
            Self {
                key,
                credential_id: uuid::Uuid::new_v4().as_bytes().to_vec(),
                count: 0,
                rp_id: rp_id.into(),
                origin: origin.into(),
            }
        }

        fn client_data(&self, kind: &str, challenge: &str) -> Vec<u8> {
            serde_json::json!({"type": kind, "challenge": challenge, "origin": self.origin, "crossOrigin": false})
                .to_string()
                .into_bytes()
        }

        fn auth_data(&self, flags: u8, with_credential: bool) -> Vec<u8> {
            let mut a = sha2::Sha256::digest(self.rp_id.as_bytes()).to_vec();
            a.push(flags);
            a.extend_from_slice(&self.count.to_be_bytes());
            if with_credential {
                a.extend_from_slice(&[0u8; 16]);
                a.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
                a.extend_from_slice(&self.credential_id);
                let point = self.key.public_key().as_ref();
                let mut k = Vec::new();
                head(&mut k, 5, 5);
                int(&mut k, 1);
                int(&mut k, 2);
                int(&mut k, 3);
                int(&mut k, -7);
                int(&mut k, -1);
                int(&mut k, 1);
                int(&mut k, -2);
                cbor_bytes(&mut k, 2, &point[1..33]);
                int(&mut k, -3);
                cbor_bytes(&mut k, 2, &point[33..65]);
                a.extend_from_slice(&k);
            }
            a
        }

        /// Risposta di registrazione: (clientDataJSON, attestationObject) in base64url.
        pub fn register(&self, challenge: &str) -> (String, String) {
            let auth = self.auth_data(UP | UV | AT, true);
            let mut att = Vec::new();
            head(&mut att, 5, 3);
            cbor_bytes(&mut att, 3, b"fmt");
            cbor_bytes(&mut att, 3, b"none");
            cbor_bytes(&mut att, 3, b"attStmt");
            head(&mut att, 5, 0);
            cbor_bytes(&mut att, 3, b"authData");
            cbor_bytes(&mut att, 2, &auth);
            (
                b64url(&self.client_data("webauthn.create", challenge)),
                b64url(&att),
            )
        }

        /// Asserzione: (clientDataJSON, authenticatorData, signature) in base64url; `uv` falso simula
        /// un autenticatore senza verifica dell'utente.
        pub fn assert(&mut self, challenge: &str, uv: bool) -> (String, String, String) {
            self.count += 1;
            let cd = self.client_data("webauthn.get", challenge);
            let auth = self.auth_data(if uv { UP | UV } else { UP }, false);
            let mut signed = auth.clone();
            signed.extend_from_slice(&sha2::Sha256::digest(&cd));
            let sig = self.key.sign(&SystemRandom::new(), &signed).unwrap();
            (b64url(&cd), b64url(&auth), b64url(sig.as_ref()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Authenticator;
    use super::*;

    const RP: &str = "jarvis.test";
    fn origins() -> Vec<String> {
        vec!["https://jarvis.test".into()]
    }

    #[test]
    fn registration_and_assertion_round_trip() {
        let mut a = Authenticator::new(RP, "https://jarvis.test");
        let (cd, att) = a.register("sfida-1");
        let reg = verify_registration(
            &from_b64url(&cd).unwrap(),
            &from_b64url(&att).unwrap(),
            "sfida-1",
            &origins(),
            RP,
        )
        .unwrap();
        assert_eq!(reg.credential_id, a.credential_id);
        assert_eq!(reg.public_key.len(), 65);
        let (cd, ad, sig) = a.assert("sfida-2", true);
        let (cd, ad, sig) = (
            from_b64url(&cd).unwrap(),
            from_b64url(&ad).unwrap(),
            from_b64url(&sig).unwrap(),
        );
        let count = verify_assertion(
            &cd,
            &ad,
            &sig,
            &reg.public_key,
            0,
            "sfida-2",
            &origins(),
            RP,
        )
        .unwrap();
        assert_eq!(count, 1);
        // Riutilizzo della stessa asserzione: contatore non cresciuto.
        assert!(verify_assertion(
            &cd,
            &ad,
            &sig,
            &reg.public_key,
            1,
            "sfida-2",
            &origins(),
            RP
        )
        .is_err());
        // Sfida, origine o RP diversi.
        assert!(
            verify_assertion(&cd, &ad, &sig, &reg.public_key, 0, "altra", &origins(), RP).is_err()
        );
        assert!(verify_assertion(
            &cd,
            &ad,
            &sig,
            &reg.public_key,
            0,
            "sfida-2",
            &["https://altro.test".into()],
            RP
        )
        .is_err());
        assert!(verify_assertion(
            &cd,
            &ad,
            &sig,
            &reg.public_key,
            0,
            "sfida-2",
            &origins(),
            "altro.test"
        )
        .is_err());
    }

    #[test]
    fn tampered_or_unverified_assertions_are_rejected() {
        let mut a = Authenticator::new(RP, "https://jarvis.test");
        let (cd, att) = a.register("r");
        let reg = verify_registration(
            &from_b64url(&cd).unwrap(),
            &from_b64url(&att).unwrap(),
            "r",
            &origins(),
            RP,
        )
        .unwrap();
        let (cd, ad, sig) = a.assert("s", true);
        let mut sig = from_b64url(&sig).unwrap();
        let last = sig.len() - 1;
        sig[last] ^= 1;
        assert!(verify_assertion(
            &from_b64url(&cd).unwrap(),
            &from_b64url(&ad).unwrap(),
            &sig,
            &reg.public_key,
            0,
            "s",
            &origins(),
            RP
        )
        .is_err());
        let (cd, ad, sig) = a.assert("s", false);
        let err = verify_assertion(
            &from_b64url(&cd).unwrap(),
            &from_b64url(&ad).unwrap(),
            &from_b64url(&sig).unwrap(),
            &reg.public_key,
            0,
            "s",
            &origins(),
            RP,
        )
        .unwrap_err();
        assert!(err.contains("verifica dell'utente"), "{err}");
    }

    #[test]
    fn cbor_rejects_malformed_input() {
        assert!(cbor(&[]).is_err());
        assert!(cbor(&[0x5f]).is_err(), "lunghezza indefinita");
        assert!(cbor(&[0x58, 10, 1]).is_err(), "troncato");
        assert_eq!(cbor(&[0x20]).unwrap().0, Cbor::Int(-1));
    }
}
