use gluon::prelude::*;
use pgpaint::{
    domain::connection_profile::ConnectionProfileRepository,
    web::{Owner, Shell, ShellCtx},
};
use serde::Serialize;

#[derive(Serialize)]
struct ConnectionCard {
    id: String,
    name: String,
    engine: String,
    host: String,
    port: u16,
    database: String,
    read_only: bool,
    color: Option<String>,
    color_style: String,
    has_saved_password: bool,
}

#[derive(Serialize)]
pub struct HomeProps {
    #[serde(flatten)]
    shell: Shell,
    connections: Vec<ConnectionCard>,
    is_empty: bool,
}

pub async fn get(
    Owner(owner): Owner,
    Inject(profiles): Inject<dyn ConnectionProfileRepository>,
    ShellCtx(mut shell): ShellCtx,
) -> gluon::Result<View<HomeProps>> {
    shell.title = "Connections · pgpaint".to_owned();
    let profiles = profiles.list(Some(&owner.0), u32::MAX).await?;
    let connections = profiles
        .into_iter()
        .map(|profile| {
            let has_saved_password = match &profile.auth {
                pgpaint::domain::connection_profile::ConnectionProfileAuth::Password {
                    encrypted_secret,
                } => encrypted_secret.is_some(),
                pgpaint::domain::connection_profile::ConnectionProfileAuth::DsqlIam { .. } => {
                    false
                }
            };
            let color = profile.color;
            let color_style = color
                .as_deref()
                .filter(|value| {
                    value.len() == 7
                        && value.starts_with('#')
                        && value.as_bytes()[1..]
                            .iter()
                            .all(u8::is_ascii_hexdigit)
                })
                .map_or_else(
                    || "background-color: #0ea5e9".to_owned(),
                    |value| format!("background-color: {value}"),
                );
            ConnectionCard {
                id: profile.id.0.to_string(),
                name: profile.name,
                engine: profile.engine.label().to_owned(),
                host: profile.host,
                port: profile.port,
                database: profile.database,
                read_only: profile.read_only,
                color,
                color_style,
                has_saved_password,
            }
        })
        .collect::<Vec<_>>();
    let is_empty = connections.is_empty();
    Ok(View::new(HomeProps {
        shell,
        connections,
        is_empty,
    }))
}