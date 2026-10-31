use gluon::prelude::*;
use pgpaint::domain::database::DatabaseGateway;
use pgpaint::web::Workspace;

pub async fn get(
    workspace: Workspace,
    Inject(database): Inject<dyn DatabaseGateway>,
) -> gluon::Result<Json<pgpaint::domain::database::CompletionSchema>> {
    if !workspace.is_connected() {
        return Err(gluon::AppError::Conflict(
            "Connect to this database before loading SQL completion.".to_owned(),
        ));
    }
    database
        .completion_schema(&workspace.key)
        .await
        .map(Json)
        .map_err(Into::into)
}
