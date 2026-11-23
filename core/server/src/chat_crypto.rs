//! Authenticated encryption for conversation fields and task documents.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};

pub(crate) const PREFIX: &str = "enc:v1:";

pub(crate) struct ChatCipher(XChaCha20Poly1305);

impl ChatCipher {
    pub(crate) fn from_secret(key: &[u8]) -> Self {
        let derived = Sha256::new()
            .chain_update(b"JARVIS conversation field encryption v1\0")
            .chain_update(key)
            .finalize();
        Self(XChaCha20Poly1305::new(&derived))
    }

    pub(crate) fn seal(&self, purpose: &str, text: &str) -> Result<String, String> {
        let mut nonce = [0; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| "Generatore casuale indisponibile")?;
        let ciphertext = self
            .0
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: text.as_bytes(),
                    aad: purpose.as_bytes(),
                },
            )
            .map_err(|_| "Cifratura della conversazione non riuscita")?;
        Ok(format!(
            "{PREFIX}{}",
            STANDARD.encode([nonce.as_slice(), &ciphertext].concat())
        ))
    }

    pub(crate) fn open(&self, purpose: &str, value: &str) -> Result<String, String> {
        let raw = value
            .strip_prefix(PREFIX)
            .and_then(|encoded| STANDARD.decode(encoded).ok())
            .filter(|raw| raw.len() >= 40)
            .ok_or("Campo cifrato non valido")?;
        let (nonce, ciphertext) = raw.split_at(24);
        let clear = self
            .0
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: purpose.as_bytes(),
                },
            )
            .map_err(|_| "Dati della conversazione alterati o chiave non valida")?;
        String::from_utf8(clear).map_err(|_| "Testo decifrato non valido".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_conversation_migrates_and_rejects_tampered_storage() {
        use crate::store::Store;
        use std::sync::Arc;

        let path = std::env::temp_dir().join(format!(
            "jarvis-cipher-test-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let mut legacy = Store::open(&path).unwrap();
        let plaintext = "enc:v1: esempio precedente";
        legacy
            .message_add("user", plaintext, "fixture", "2026-01-01", None, None)
            .unwrap();
        legacy
            .task_put(
                &serde_json::json!({"task_id":"fixture","objective":plaintext}),
                "2026-01-01",
            )
            .unwrap();
        let cipher = Arc::new(ChatCipher::from_secret(&[17; 32]));
        legacy.protect_chat(cipher.clone()).unwrap();
        assert_eq!(legacy.messages_recent(1).unwrap()[0]["content"], plaintext);
        assert_eq!(legacy.tasks_recent(1).unwrap()[0]["objective"], plaintext);
        let raw: String = legacy
            .conn
            .query_row("SELECT content FROM messages", [], |r| r.get(0))
            .unwrap();
        assert!(raw.starts_with(PREFIX));
        assert!(!raw.contains(plaintext));
        drop(legacy);
        let mut reopened = Store::open(&path).unwrap();
        reopened.protect_chat(cipher).unwrap();
        assert_eq!(
            reopened.messages_recent(1).unwrap()[0]["content"],
            plaintext
        );
        reopened
            .conn
            .execute("UPDATE messages SET role='assistant'", [])
            .unwrap();
        assert!(reopened.messages_recent(1).is_err());
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }
}
