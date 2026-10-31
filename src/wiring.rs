use gluon::ContainerBuilder;

#[allow(unused_mut)]
#[must_use]
pub fn build_container(builder: ContainerBuilder) -> ContainerBuilder {
    let mut builder = builder;
    // <gluon:binds>
    // <gluon:bind:domain:database_gateway>
    builder = builder.bind::<dyn crate::domain::database::DatabaseGateway, _>(|_| {
        std::sync::Arc::new(crate::infrastructure::engines::gateway::SqlxGateway::new())
    });
    // </gluon:bind:domain:database_gateway>
    // <gluon:bind:domain:connection_profile>
    builder = builder.bind::<dyn crate::domain::connection_profile::ConnectionProfileRepository, _>(
        |c| {
            std::sync::Arc::new(
                crate::infrastructure::persistence::connection_profile_repository::PostgresConnectionProfileRepository::new(
                    c.resolve::<sqlx::PgPool>(),
                ),
            )
        },
    );
    // </gluon:bind:domain:connection_profile>
    // <gluon:bind:domain:query_history>
    builder = builder.bind::<dyn crate::domain::query_history::QueryHistoryRepository, _>(|c| {
        std::sync::Arc::new(
            crate::infrastructure::persistence::query_history_repository::PostgresQueryHistoryRepository::new(
                c.resolve::<sqlx::PgPool>(),
            ),
        )
    });
    // </gluon:bind:domain:query_history>
    // <gluon:bind:domain:query_run>
    builder = builder.bind::<dyn crate::domain::query_run::QueryRunStore, _>(|_| {
        std::sync::Arc::new(crate::infrastructure::query_run_store::InMemoryQueryRunStore::new())
    });
    // </gluon:bind:domain:query_run>
    // <gluon:bind:domain:saved_query>
    builder = builder.bind::<dyn crate::domain::saved_query::SavedQueryRepository, _>(|c| {
        std::sync::Arc::new(
            crate::infrastructure::persistence::saved_query_repository::PostgresSavedQueryRepository::new(
                c.resolve::<sqlx::PgPool>(),
            ),
        )
    });
    // </gluon:bind:domain:saved_query>
    // <gluon:bind:domain:secret_cipher>
    builder = builder.bind::<dyn crate::domain::connection_profile::SecretCipher, _>(|_| {
        std::sync::Arc::new(crate::infrastructure::crypto::AesGcmSecretCipher::from_env())
    });
    // </gluon:bind:domain:secret_cipher>
    // <gluon:bind:usecase:close_connection>
    builder = builder.bind::<dyn crate::usecases::close_connection::CloseConnection, _>(|c| {
        std::sync::Arc::new(crate::usecases::close_connection::CloseConnectionImpl::new(
            c.resolve::<dyn crate::domain::database::DatabaseGateway>(),
        ))
    });
    // </gluon:bind:usecase:close_connection>
    // <gluon:bind:usecase:open_connection>
    builder = builder.bind::<dyn crate::usecases::open_connection::OpenConnection, _>(|c| {
        std::sync::Arc::new(crate::usecases::open_connection::OpenConnectionImpl::new(
            c.resolve::<dyn crate::domain::connection_profile::ConnectionProfileRepository>(),
            c.resolve::<dyn crate::domain::connection_profile::SecretCipher>(),
            c.resolve::<dyn crate::domain::database::DatabaseGateway>(),
        ))
    });
    // </gluon:bind:usecase:open_connection>
    // </gluon:binds>
    builder
}
