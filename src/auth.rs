use std::{error::Error, sync::Arc, time::Duration};

use gluon::auth::{
    AuthBuilder, AuthConfig,
    oauth::{GitHubProvider, GoogleProvider, OAuthClientConfig, OidcProvider, ReqwestOAuthClient},
};

#[derive(Debug, Clone, Default)]
pub struct AuthUi {
    pub email_otp: bool,
    pub passkeys: bool,
    pub oauth_providers: Vec<String>,
}

/// Builds the optional shared-server authentication configuration from environment variables.
///
/// # Errors
/// Returns an error for incomplete or invalid configuration, including provider discovery failures.
pub async fn from_env() -> Result<(Option<AuthConfig>, AuthUi), Box<dyn Error + Send + Sync>> {
    if setting("PGPAINT_AUTH")?.as_deref() != Some("1") {
        return Ok((None, AuthUi::default()));
    }

    let origin = setting("AUTH_ORIGIN")?;
    let (builder, email_otp) = email_builder()?;
    let mut config = builder.build()?;
    config.migrate_on_start = true;
    if let Some(origin) = &origin {
        config.allowed_origins.push(origin.clone());
    }
    let passkeys = configure_passkeys(&mut config, origin.as_deref())?;
    let oauth_providers = configure_oauth(&mut config, origin.as_deref()).await?;
    if !email_otp && !passkeys && oauth_providers.is_empty() {
        return Err("PGPAINT_AUTH=1 requires a configured sign-in method".into());
    }

    Ok((
        Some(config),
        AuthUi {
            email_otp,
            passkeys,
            oauth_providers,
        },
    ))
}

fn email_builder() -> Result<(AuthBuilder, bool), Box<dyn Error + Send + Sync>> {
    let server = setting("AUTH_SMTP_SERVER")?;
    let port = setting("AUTH_SMTP_PORT")?;
    let username = setting("AUTH_SMTP_USERNAME")?;
    let password = setting("AUTH_SMTP_PASSWORD")?;
    let from = setting("AUTH_SMTP_FROM")?;
    let otp_secret = setting("AUTH_OTP_SECRET")?;
    let configured = [
        server.is_some(),
        port.is_some(),
        username.is_some(),
        password.is_some(),
        from.is_some(),
    ]
    .into_iter()
    .any(|value| value);
    if !configured {
        if otp_secret.is_some() {
            return Err("AUTH_OTP_SECRET requires SMTP configuration".into());
        }
        return Ok((AuthBuilder::new(), false));
    }

    let sender = Arc::new(gluon::auth::SmtpEmailSender::new(
        gluon::auth::SmtpConfig {
            server: required(server, "AUTH_SMTP_SERVER")?,
            port: port.map_or(Ok(587), |value| value.parse::<u16>())?,
            username: required(username, "AUTH_SMTP_USERNAME")?,
            password: required(password, "AUTH_SMTP_PASSWORD")?,
            from: required(from, "AUTH_SMTP_FROM")?,
        },
    )?);
    let secret = required(otp_secret, "AUTH_OTP_SECRET")?;
    Ok((
        AuthBuilder::new().email_otp(sender, secret.into_bytes()),
        true,
    ))
}

fn configure_passkeys(
    config: &mut AuthConfig,
    origin: Option<&str>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    let Some(rp_id) = setting("AUTH_RP_ID")? else {
        return Ok(false);
    };
    let origin = required(
        origin.map(str::to_owned),
        "AUTH_ORIGIN (required with AUTH_RP_ID)",
    )?;
    config.passkey = Some(gluon::auth::PasskeyConfig { rp_id, origin });
    Ok(true)
}

async fn configure_oauth(
    config: &mut AuthConfig,
    origin: Option<&str>,
) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
    let http = Arc::new(ReqwestOAuthClient::new(
        Duration::from_secs(10),
        1024 * 1024,
    ));
    let mut providers = Vec::new();
    if let Some((client_id, client_secret)) =
        provider_credentials("PGPAINT_GOOGLE_CLIENT_ID", "PGPAINT_GOOGLE_CLIENT_SECRET")?
    {
        let provider = GoogleProvider::new(
            oauth_client(
                client_id,
                client_secret,
                callback(origin, "google")?,
                config,
            ),
            http.clone(),
        )
        .await?;
        config.oauth.push(Arc::new(provider));
        providers.push("google".to_owned());
    }
    if let Some((client_id, client_secret)) =
        provider_credentials("PGPAINT_GITHUB_CLIENT_ID", "PGPAINT_GITHUB_CLIENT_SECRET")?
    {
        let provider = GitHubProvider::new(
            oauth_client(
                client_id,
                client_secret,
                callback(origin, "github")?,
                config,
            ),
            http.clone(),
        )?;
        config.oauth.push(Arc::new(provider));
        providers.push("github".to_owned());
    }

    let issuer = setting("PGPAINT_OIDC_ISSUER")?;
    let client_id = setting("PGPAINT_OIDC_CLIENT_ID")?;
    let client_secret = setting("PGPAINT_OIDC_CLIENT_SECRET")?;
    let provider_name = setting("PGPAINT_OIDC_NAME")?;
    let ca_file = setting("PGPAINT_OIDC_CA_FILE")?;
    let configured = issuer.is_some()
        || client_id.is_some()
        || client_secret.is_some()
        || provider_name.is_some()
        || ca_file.is_some();
    if configured {
        let issuer = required(issuer, "PGPAINT_OIDC_ISSUER")?;
        let client_id = required(client_id, "PGPAINT_OIDC_CLIENT_ID")?;
        let client_secret = required(client_secret, "PGPAINT_OIDC_CLIENT_SECRET")?;
        let name = provider_name.unwrap_or_else(|| "oidc".to_owned());
        let oidc_transport = if let Some(path) = ca_file {
            let pem_bundle = std::fs::read(path)?;
            Arc::new(ReqwestOAuthClient::new_with_root_certificates(
                Duration::from_secs(10),
                1024 * 1024,
                &pem_bundle,
            )?)
        } else {
            http
        };
        let provider = OidcProvider::discover(
            &name,
            &issuer,
            oauth_client(client_id, client_secret, callback(origin, &name)?, config),
            oidc_transport,
        )
        .await?;
        config.oauth.push(Arc::new(provider));
        providers.push(name);
    }
    Ok(providers)
}

fn oauth_client(
    client_id: String,
    client_secret: String,
    callback_url: String,
    config: &AuthConfig,
) -> OAuthClientConfig {
    OAuthClientConfig {
        client_id,
        client_secret,
        callback_url,
        clock: config.clock.clone(),
    }
}

fn callback(origin: Option<&str>, provider: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    let origin = required(
        origin.map(str::to_owned),
        "AUTH_ORIGIN (required with OAuth providers)",
    )?;
    Ok(format!(
        "{}/oauth/{provider}/callback",
        origin.trim_end_matches('/')
    ))
}

fn provider_credentials(
    id_name: &str,
    secret_name: &str,
) -> Result<Option<(String, String)>, Box<dyn Error + Send + Sync>> {
    let id = setting(id_name)?;
    let secret = setting(secret_name)?;
    if id.is_some() || secret.is_some() {
        Ok(Some((
            required(id, id_name)?,
            required(secret, secret_name)?,
        )))
    } else {
        Ok(None)
    }
}

fn setting(name: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn required(value: Option<String>, name: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    value.ok_or_else(|| {
        format!("{name} is required for the configured authentication method").into()
    })
}
