use gluon::prelude::*;
use pgpaint::{auth::AuthUi, web::{Shell, ShellCtx}};
use serde::Serialize;

#[derive(Serialize)]
pub struct LoginProps {
    #[serde(flatten)]
    shell: Shell,
    email_otp: bool,
    passkeys: bool,
    oauth_providers: Vec<String>,
}

pub async fn get(Inject(auth): Inject<AuthUi>, ShellCtx(mut shell): ShellCtx) -> gluon::Result<View<LoginProps>> {
    shell.title = "Sign in · pgpaint".to_owned();
    Ok(View::new(LoginProps {
        shell,
        email_otp: auth.email_otp,
        passkeys: auth.passkeys,
        oauth_providers: auth.oauth_providers.clone(),
    }))
}
