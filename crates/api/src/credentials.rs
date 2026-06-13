use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct CredentialCipher {
    key: [u8; 32],
}

impl CredentialCipher {
    pub fn from_base64(value: &str) -> anyhow::Result<Self> {
        let decoded = STANDARD
            .decode(value.trim())
            .map_err(|_| anyhow::anyhow!("BELLA_CREDENTIAL_ENCRYPTION_KEY must be base64"))?;
        let key: [u8; 32] = decoded.try_into().map_err(|_| {
            anyhow::anyhow!("BELLA_CREDENTIAL_ENCRYPTION_KEY must decode to exactly 32 bytes")
        })?;
        Ok(Self { key })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> anyhow::Result<(Vec<u8>, [u8; 12])> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| anyhow::anyhow!("invalid credential encryption key"))?;
        let mut nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| anyhow::anyhow!("credential encryption failed"))?;
        Ok((ciphertext, nonce))
    }
}

pub fn fingerprint(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    format!("{:x}", digest)[..8].to_owned()
}

#[cfg(test)]
mod tests {
    use super::{CredentialCipher, fingerprint};
    use base64::{Engine, engine::general_purpose::STANDARD};

    #[test]
    fn encrypts_with_unique_nonces_and_stable_fingerprints() {
        let cipher = CredentialCipher::from_base64(&STANDARD.encode([7_u8; 32])).unwrap();
        let (first, first_nonce) = cipher.encrypt(b"secret").unwrap();
        let (_second, second_nonce) = cipher.encrypt(b"secret").unwrap();

        assert_ne!(first_nonce, second_nonce);
        assert_ne!(first, b"secret");
        assert_eq!(fingerprint("secret"), fingerprint("secret"));
    }
}
