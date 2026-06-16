pub use bella_ingestion::credentials::{CredentialCipher, fingerprint};

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
