use super::value_objects::EncryptedSecret;
use crate::domain::database::Secret;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretCipherError {
    MissingKey,
    InvalidEncoding,
    UnsupportedVersion,
    AuthenticationFailed,
    EncryptionFailed,
}

impl fmt::Display for SecretCipherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingKey => "SECRET_KEY_BASE is required to save passwords.",
            Self::InvalidEncoding => "Encrypted credential has an invalid encoding.",
            Self::UnsupportedVersion => "Encrypted credential uses an unsupported version.",
            Self::AuthenticationFailed => "Encrypted credential could not be authenticated.",
            Self::EncryptionFailed => "Credential encryption failed.",
        };
        f.write_str(message)
    }
}

impl std::error::Error for SecretCipherError {}

/// Encrypts credentials before they are persisted with connection profiles.
pub trait SecretCipher: Send + Sync {
    /// Encrypts a plaintext database credential for metadata storage.
    ///
    /// # Errors
    /// Returns [`SecretCipherError::MissingKey`] when `SECRET_KEY_BASE` is unavailable.
    fn encrypt(&self, secret: &Secret) -> Result<EncryptedSecret, SecretCipherError>;
    /// Decrypts a profile credential before it is used to connect.
    ///
    /// # Errors
    /// Returns an error when the ciphertext is invalid or cannot be authenticated.
    fn decrypt(&self, encrypted: &EncryptedSecret) -> Result<Secret, SecretCipherError>;
}
