use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionProfileId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedSecret(String);

impl EncryptedSecret {
    #[must_use]
    pub fn new(ciphertext: String) -> Self {
        Self(ciphertext)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionProfileAuth {
    Password {
        encrypted_secret: Option<EncryptedSecret>,
    },
    DsqlIam {
        region: String,
        admin: bool,
    },
}
