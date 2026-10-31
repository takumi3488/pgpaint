use gluon::prelude::*;
use pgpaint::{auth::AuthUi, web::{Owner, Shell, ShellCtx}};
use serde::Serialize;

#[derive(Serialize)]
pub struct PasskeyProps {
    #[serde(flatten)]
    shell: Shell,
}

pub async fn get(
    Owner(_owner): Owner,
    Inject(auth): Inject<AuthUi>,
    ShellCtx(mut shell): ShellCtx,
) -> gluon::Result<View<PasskeyProps>> {
    if !auth.passkeys {
        return Err(AppError::NotFound);
    }
    shell.title = "Passkeys · pgpaint".to_owned();
    Ok(View::new(PasskeyProps { shell }))
}
