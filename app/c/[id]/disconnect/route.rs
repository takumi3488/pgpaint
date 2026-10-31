use axum::extract::Path;
use gluon::prelude::*;
use pgpaint::domain::connection_profile::ConnectionProfileId;
use pgpaint::usecases::close_connection::CloseConnection;
use pgpaint::web::{manual_disconnect_key, Owner};

pub async fn post(
    Path(id): Path<String>,
    Owner(owner): Owner,
    session: Session,
    Inject(close_connection): Inject<dyn CloseConnection>,
) -> gluon::Result<Redirect> {
    let profile_id = uuid::Uuid::parse_str(&id)
        .map(ConnectionProfileId)
        .map_err(|_| gluon::AppError::NotFound)?;
    close_connection.execute(owner, profile_id.clone()).await;
    session
        .insert(&manual_disconnect_key(&profile_id), true)
        .await
        .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
    Flash::set(&session, "notice", "Disconnected.")
        .await
        .map_err(|error| gluon::AppError::Internal(Box::new(error)))?;
    Ok(Redirect::to(format!("/c/{id}")))
}
