use super::value_objects::SavedQueryId;
use crate::domain::connection_profile::ConnectionProfileId;
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedQuery {
    pub id: SavedQueryId,
    pub owner: Option<String>,
    pub profile_id: Option<ConnectionProfileId>,
    pub name: String,
    pub sql: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
