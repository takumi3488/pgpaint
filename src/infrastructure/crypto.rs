use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::domain::connection_profile::{EncryptedSecret, SecretCipher, SecretCipherError};
use crate::domain::database::Secret;

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const KEY_INFO: &[u8] = b"pgpaint/credentials/v1";
const FORMAT_VERSION: &str = "v1:";

pub struct AesGcmSecretCipher {
    key: Option<[u8; 32]>,
}

impl AesGcmSecretCipher {
    #[must_use]
    pub fn new(secret_key_base: Option<&str>) -> Self {
        let key = secret_key_base
            .filter(|key| !key.is_empty())
            .and_then(|secret| {
                let hkdf = Hkdf::<Sha256>::new(None, secret.as_bytes());
                let mut key = [0_u8; 32];
                hkdf.expand(KEY_INFO, &mut key).ok().map(|()| key)
            });
        Self { key }
    }

    #[must_use]
    pub fn from_env() -> Self {
        Self::new(std::env::var("SECRET_KEY_BASE").ok().as_deref())
    }
}

impl SecretCipher for AesGcmSecretCipher {
    fn encrypt(&self, secret: &Secret) -> Result<EncryptedSecret, SecretCipherError> {
        let key = self.key.as_ref().ok_or(SecretCipherError::MissingKey)?;
        let cipher =
            Aes256Gcm::new_from_slice(key).map_err(|_| SecretCipherError::EncryptionFailed)?;
        let mut nonce = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| SecretCipherError::EncryptionFailed)?;
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: secret.expose().as_bytes(),
                    aad: FORMAT_VERSION.as_bytes(),
                },
            )
            .map_err(|_| SecretCipherError::EncryptionFailed)?;
        let mut encoded =
            String::with_capacity(FORMAT_VERSION.len() + (NONCE_LEN + ciphertext.len()) * 2);
        encoded.push_str(FORMAT_VERSION);
        encode_hex(&nonce, &mut encoded);
        encode_hex(&ciphertext, &mut encoded);
        Ok(EncryptedSecret::new(encoded))
    }

    fn decrypt(&self, encrypted: &EncryptedSecret) -> Result<Secret, SecretCipherError> {
        let key = self.key.as_ref().ok_or(SecretCipherError::MissingKey)?;
        let payload = encrypted
            .expose()
            .strip_prefix(FORMAT_VERSION)
            .ok_or_else(|| {
                if encrypted.expose().starts_with('v') {
                    SecretCipherError::UnsupportedVersion
                } else {
                    SecretCipherError::InvalidEncoding
                }
            })?;
        let bytes = decode_hex(payload)?;
        if bytes.len() < NONCE_LEN + TAG_LEN {
            return Err(SecretCipherError::InvalidEncoding);
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce).map_err(|_| SecretCipherError::InvalidEncoding)?;
        let cipher =
            Aes256Gcm::new_from_slice(key).map_err(|_| SecretCipherError::AuthenticationFailed)?;
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: FORMAT_VERSION.as_bytes(),
                },
            )
            .map_err(|_| SecretCipherError::AuthenticationFailed)?;
        let plaintext =
            String::from_utf8(plaintext).map_err(|_| SecretCipherError::InvalidEncoding)?;
        Ok(Secret::new(plaintext))
    }
}

fn encode_hex(bytes: &[u8], output: &mut String) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, SecretCipherError> {
    if !value.len().is_multiple_of(2) {
        return Err(SecretCipherError::InvalidEncoding);
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = hex_nibble(pair[0]).ok_or(SecretCipherError::InvalidEncoding)?;
            let low = hex_nibble(pair[1]).ok_or(SecretCipherError::InvalidEncoding)?;
            Ok((high << 4) | low)
        })
        .collect()
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encrypt(cipher: &AesGcmSecretCipher, value: &str) -> EncryptedSecret {
        match cipher.encrypt(&Secret::new(value.to_owned())) {
            Ok(encrypted) => encrypted,
            Err(error) => panic!("unexpected encryption error: {error}"),
        }
    }

    #[test]
    fn roundtrips_secret() {
        let cipher = AesGcmSecretCipher::new(Some("stable key material"));
        let encrypted = encrypt(&cipher, "correct horse battery staple");
        let decrypted = match cipher.decrypt(&encrypted) {
            Ok(secret) => secret,
            Err(error) => panic!("unexpected decryption error: {error}"),
        };
        assert_eq!(decrypted.expose(), "correct horse battery staple");
        assert!(encrypted.expose().starts_with(FORMAT_VERSION));
    }

    #[test]
    fn rejects_tampering_and_wrong_keys() {
        let cipher = AesGcmSecretCipher::new(Some("stable key material"));
        let encrypted = encrypt(&cipher, "password");
        let mut tampered = encrypted.expose().to_owned();
        let index = FORMAT_VERSION.len() + 24;
        let replacement = if tampered.as_bytes()[index] == b'0' {
            '1'
        } else {
            '0'
        };
        tampered.replace_range(index..=index, &replacement.to_string());
        assert!(cipher.decrypt(&EncryptedSecret::new(tampered)).is_err());

        let wrong_key = AesGcmSecretCipher::new(Some("different key material"));
        assert!(wrong_key.decrypt(&encrypted).is_err());
    }

    #[test]
    fn refuses_encryption_without_secret_key_base() {
        let cipher = AesGcmSecretCipher::new(None);
        assert_eq!(
            cipher.encrypt(&Secret::new("password".to_owned())).err(),
            Some(SecretCipherError::MissingKey),
        );
    }
}
