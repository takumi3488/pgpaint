use anyhow::{Context, Result};
use pgpaint_testkit::{
    Authentication, cockroach, dsql, metadata_postgres, postgres, seed, yugabyte,
};
use tokio::signal;

#[tokio::main]
async fn main() -> Result<()> {
    let (metadata, postgres, cockroach, yugabyte, dsql) = tokio::try_join!(
        metadata_postgres(),
        postgres(),
        cockroach(),
        yugabyte(),
        dsql(),
    )?;
    println!(
        "pgpaint metadata: DATABASE_URL={}",
        metadata.connection.database_url()
    );
    print_connection("postgres", &postgres.connection);
    print_connection("cockroach", &cockroach.connection);
    print_connection("yugabyte", &yugabyte.connection);
    print_connection("dsql", &dsql.connection);

    tokio::try_join!(
        seed(&postgres),
        seed(&cockroach),
        seed(&yugabyte),
        seed(&dsql),
    )?;
    println!("Seeded all engine databases. Press Ctrl-C to stop the devstack.");
    signal::ctrl_c()
        .await
        .context("waiting for Ctrl-C to stop containers")?;
    println!("Stopping devstack containers.");
    dsql.shutdown()
        .await
        .context("shutting down DSQL cluster")?;
    Ok(())
}

fn print_connection(name: &str, connection: &pgpaint_testkit::ConnectionParams) {
    println!(
        "{name}: host={} port={} database={} user={} url={}",
        connection.host,
        connection.port,
        connection.database,
        connection.user,
        connection.database_url()
    );
    match &connection.auth {
        Authentication::Password { password } if !password.is_empty() => {
            println!("  password={password}");
        }
        Authentication::DsqlIam {
            region,
            control_plane_endpoint,
            access_key_id,
            secret_access_key,
        } => {
            println!(
                "  IAM=DbConnectAdmin region={region} access_key_id={access_key_id} \
                 secret_access_key={secret_access_key} control_plane={control_plane_endpoint}"
            );
            if let Some(tls) = &connection.tls {
                match &tls.ca_certificate_pem {
                    Some(ca_pem) => {
                        println!("  TLS={:?} CA_CERTIFICATE_PEM=\n{ca_pem}", tls.mode);
                    }
                    None => println!("  TLS={:?} CA_CERTIFICATE_PEM=<missing>", tls.mode),
                }
            }
        }
        Authentication::Password { .. } => {}
    }
}
