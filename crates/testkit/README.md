# pgpaint-testkit

Async Testcontainers starters for pgpaint's PostgreSQL-compatible engine tests and local devstack.

```rust
use pgpaint_testkit::{metadata_postgres, postgres, seed};

let (metadata, postgres) = tokio::try_join!(metadata_postgres(), postgres())?;
println!("DATABASE_URL={}", metadata.connection.database_url());
seed(&postgres).await?;
```

Dropped `Database` values schedule best-effort container cleanup. Run tests using the testkit on a Tokio multi-thread runtime: testcontainers 0.27 synchronously waits for async Docker cleanup from `Drop`, which can stall a current-thread runtime. Prefer awaiting `database.shutdown()` at the end of async tests and devstack runs; for DSQL this calls Floci `DeleteCluster`, waits for `floci-aws-dsql-<cluster-id>` to disappear, then uses the Docker API as a forced-removal fallback. `Database.connection` exposes the host, mapped port, database, user, authentication, and optional TLS/CA details. `database_url()` is directly usable for password-auth databases; DSQL requires a fresh IAM token and its CA certificate.

## Local OIDC provider

`dex(callback_uri)` starts a disposable HTTPS Dex OIDC server with a static browser-login user and client. Its returned `issuer_url`, `client_id`, and `client_secret` configure an OIDC test client; use its `email` and `password` for the Dex login form (`username` is the OIDC display name). Pass the app's exact registered HTTPS callback URI. The generated test CA is exposed as `ca_certificate_pem` and `ca_certificate_path`; set pgpaint's `PGPAINT_OIDC_CA_FILE` to the latter and trust the CA in the browser used for the test. The generated HTTPS certificate and key are also available for a local app TLS proxy. `PGPAINT_TESTCONTAINERS_HOST`, when set, is used in the issuer URL and certificate SAN so the process and browser can reach the mapped port. Do not expose the test provider publicly.


## Docker daemon host override

Set `PGPAINT_TESTCONTAINERS_HOST` when the process running pgpaint is not on the Docker daemon host's network namespace. The value is used as the connection host for metadata PostgreSQL, PostgreSQL, CockroachDB, and YugabyteDB, and for Floci's control-plane URL. For DSQL, the TLS/SNI hostname remains Floci's `*.dsql.<region>.localhost.floci.io`; a local TCP forwarder owned by the returned handle bridges its mapped port to `PGPAINT_TESTCONTAINERS_HOST`.

For example, when a macOS VM reaches an OrbStack daemon through a forwarded socket, set `PGPAINT_TESTCONTAINERS_HOST` to the host Mac address. Testcontainers still uses `DOCKER_HOST` to reach the Docker API, and Floci receives `/var/run/docker.sock` inside its container.

## Images and startup

- PostgreSQL and metadata database: `postgres:18.6-trixie`.
- CockroachDB: `cockroachdb/cockroach:v25.2.26`, single-node insecure mode.
- YugabyteDB: `yugabytedb/yugabyte:2025.2.8.0-b60`, `bin/yugabyted start --background=false`, YSQL 5433. Readiness polls PostgreSQL connectivity for up to ten minutes.
- Floci/DSQL: `floci/floci:nightly-10292026`. Stable Floci `2.3.0` rejects `CreateCluster` as an unsupported DSQL service scope; the dated nightly is pinned until DSQL support is released. Floci creates a DSQL cluster through the AWS DSQL API with dummy local credentials, exposes the CA PEM, and uses a fresh admin IAM token for seed connections.

The selected `testcontainers-modules 0.15.0` requires Testcontainers 0.27.x, so the testkit uses its re-exported 0.27.3 API. First-run image downloads dominate startup; YugabyteDB and DSQL may take minutes. `cargo run -p pgpaint-testkit --example devstack` starts metadata PostgreSQL plus all four engines concurrently, seeds the four engines, prints connection details, and waits for Ctrl-C.
