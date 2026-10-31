# pgpaint

pgpaint is a web-based database workspace for PostgreSQL-compatible engines. It keeps connection profiles and query history in a PostgreSQL metadata database, while letting you connect to PostgreSQL, Aurora DSQL, CockroachDB, and YugabyteDB.

## Features

- Save and test database connection profiles, set a profile read-only, and optionally save passwords encrypted with `SECRET_KEY_BASE`.
- Explore schemas, tables, columns, indexes, constraints, relationships, and table DDL.
- Browse, filter, and edit table rows; edit operations require a primary key.
- Run SQL with schema-aware completion, inspect result sets, cancel running queries, and export results as CSV or JSON.
- Review EXPLAIN plans, view query history, and save queries.
- Draw ER diagrams, including inferred relationships where foreign keys are absent.
- Inspect engine-specific views, including PostgreSQL activity and locks, CockroachDB ranges/jobs/regions/sessions, YugabyteDB servers/activity/tablets/table properties, and Aurora DSQL jobs.
- Optional shared authentication with email OTP, Google/GitHub OAuth, generic OIDC, and passkeys.

## Supported databases

| Engine | Authentication | Default port |
| --- | --- | ---: |
| PostgreSQL | Password | 5432 |
| Aurora DSQL | IAM | 5432 |
| CockroachDB | Password | 26257 |
| YugabyteDB (YSQL) | Password | 5433 |

Aurora DSQL connections use IAM authentication, not a database password. pgpaint obtains credentials from the AWS SDK default credential chain available to the pgpaint process; configure the AWS identity and permissions there. Enter the DSQL region on the connection profile. pgpaint requests IAM tokens as needed and enforces TLS full hostname verification. When the endpoint's CA is not in the system trust store (for example, with Floci), paste its root CA PEM into the profile's **Root certificate PEM** field; pgpaint has no CA environment variable. The DSQL profile can use the configured database username or the DSQL `admin` role.

For the other engines, connection profiles support password authentication and configurable SSL modes. `DATABASE_URL` below is for pgpaint's own metadata database; it is separate from the databases you add as profiles.

## Quick start

pgpaint needs a PostgreSQL database for its metadata (profiles, saved queries, query history, and sessions). Point `DATABASE_URL` at an existing database. pgpaint applies its metadata migrations when it starts.

Generate a secret key for secure session cookies and saved-password encryption:

```sh
openssl rand -base64 64
```

Set the output as `SECRET_KEY_BASE`. For local HTTP development, the examples below also set `GLUON_INSECURE_COOKIE=1`; do not use that setting in production.

### Binary

Download the archive for your platform from [GitHub Releases](https://github.com/takumi3488/pgpaint/releases), then start pgpaint with the metadata URL and secret:

```sh
DATABASE_URL='postgres://user:password@db-host:5432/pgpaint' \
SECRET_KEY_BASE='replace-with-at-least-64-random-bytes' \
GLUON_BIND=127.0.0.1:3000 \
GLUON_INSECURE_COOKIE=1 \
./pgpaint
```

Open <http://127.0.0.1:3000>.

### Docker

Build the image from the repository. The Dockerfile builds the web assets and packages them with the pgpaint binary:

```sh
docker build -t pgpaint .
docker run --rm \
  -p 127.0.0.1:3000:3000 \
  -e DATABASE_URL='postgres://user:password@reachable-db-host:5432/pgpaint' \
  -e SECRET_KEY_BASE='replace-with-at-least-64-random-bytes' \
  -e GLUON_INSECURE_COOKIE=1 \
  pgpaint
```

The database host must be reachable from inside the container; `localhost` inside the container refers to the container itself, not your host. Open <http://127.0.0.1:3000>.

### From source

Build the browser assets, then run the Rust application:

```sh
cd web
bun install --frozen-lockfile
bun run build
cd ..

DATABASE_URL='postgres://user:password@localhost:5432/pgpaint' \
SECRET_KEY_BASE='replace-with-at-least-64-random-bytes' \
GLUON_BIND=127.0.0.1:3000 \
GLUON_INSECURE_COOKIE=1 \
cargo run
```

The `SECRET_KEY_BASE` shown here is a placeholder; replace it with a randomly generated value before use.

## Configuration

The app and its test/development tools consume the following environment variables.

| Variable | Purpose |
| --- | --- |
| `DATABASE_URL` | Required PostgreSQL URL for pgpaint metadata and persistent sessions. |
| `SECRET_KEY_BASE` | At least 64 random bytes; signs session cookies and enables encrypted saved passwords. Without it, saved-password storage is disabled. Required for secure cookies unless the development-only insecure-cookie setting is enabled. |
| `GLUON_BIND` | Server bind address, in `host:port` form. Overrides `gluon.toml`; the current default is `0.0.0.0:3000`. Use `127.0.0.1:3000` for local-only access. |
| `GLUON_INSECURE_COOKIE` | Set to `1` or `true` only for local HTTP development to omit the cookie's `Secure` attribute. Leave unset in production. |
| `GLUON_DEV_TEMPLATES` | Set to `1` to load TSX templates and public assets from disk for live development edits. Otherwise the compiled app uses embedded templates/assets. |
| `PGPAINT_CONNECTION_IDLE_TTL_SECS` | Positive number of seconds an unused profile connection pool is retained before eviction; defaults to 1800 seconds. |
| `OTEL_ENABLED` | Enables telemetry when set to `1` or `true`; any other set value disables it. If unset, telemetry is enabled by default. |
| `GLUON_TELEMETRY_DISABLED` | Set to `1` or `true` to disable OpenTelemetry regardless of `OTEL_ENABLED`. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP gRPC collector URL; defaults to `http://localhost:4317`. |
| `OTEL_SERVICE_NAME` | Overrides the telemetry service name; defaults to `pgpaint`. |
| `RUST_LOG` | Tracing filter; defaults to `info`. |
| `PGPAINT_TESTCONTAINERS_HOST` | Test/development override for the host used to reach Testcontainers services when pgpaint and the Docker daemon are in different network namespaces. |
| `DOCKER_HOST` | Docker API socket or endpoint used by Testcontainers when the daemon is not at its default location. |
| `PGPAINT_SKIP_DSQL_TESTS` | Test-only: any set value skips the DSQL branch of `tests/introspection.rs`; other DSQL tests still run. |
| `PGPAINT_CAPTURE_EXPLAIN_FIXTURES` | Test-only: any set value overwrites raw EXPLAIN output fixtures under `tests/fixtures/explain/` when `tests/explain_views.rs` runs. |
| `PGPAINT_AUTH` | Set exactly to `1` to enable shared-mode authentication; otherwise local single-user mode remains enabled. At least one complete sign-in method must be configured: SMTP plus OTP secret, passkeys, Google/GitHub OAuth, or OIDC. |
| `AUTH_ORIGIN` | Optional unless OAuth or passkeys are enabled. Set it to the public application origin, for example `https://pgpaint.example.com`. |
| `AUTH_RP_ID` | WebAuthn relying-party ID (for example, `pgpaint.example.com`); requires `AUTH_ORIGIN` and must be an effective domain of that origin. |
| `AUTH_SMTP_SERVER`, `AUTH_SMTP_PORT`, `AUTH_SMTP_USERNAME`, `AUTH_SMTP_PASSWORD`, `AUTH_SMTP_FROM` | Configure the TLS SMTP sender for email OTP. `AUTH_SMTP_PORT` defaults to `587`; set the complete group to enable email OTP. |
| `AUTH_OTP_SECRET` | High-entropy OTP HMAC secret of at least 32 bytes; required with email OTP configuration. |
| `PGPAINT_GOOGLE_CLIENT_ID`, `PGPAINT_GOOGLE_CLIENT_SECRET` | Google OAuth client pair; configure both or neither. |
| `PGPAINT_GITHUB_CLIENT_ID`, `PGPAINT_GITHUB_CLIENT_SECRET` | GitHub OAuth client pair; configure both or neither. |
| `PGPAINT_OIDC_ISSUER`, `PGPAINT_OIDC_CLIENT_ID`, `PGPAINT_OIDC_CLIENT_SECRET` | Generic OIDC provider configuration; configure the complete group. |
| `PGPAINT_OIDC_NAME` | Optional OIDC provider name used in its routes and callback URL; defaults to `oidc`. |
| `PGPAINT_OIDC_CA_FILE` | Optional PEM bundle path for generic OIDC only; requires the complete OIDC configuration. Loaded at startup and added to platform roots without replacing them; has no effect for Google/GitHub. Useful for a private issuer or test CA. |

Partial SMTP or identity-provider configuration is rejected at startup. When shared authentication is enabled, pgpaint applies the Gluon auth schema migrations at startup. OAuth flows start with `POST /auth/oauth/{provider}/start`; register the app-owned callback URL as `{AUTH_ORIGIN}/oauth/{provider}/callback` (replace `{provider}` with `google`, `github`, or the configured OIDC provider name). DSQL IAM credentials are not configured with a pgpaint-specific secret variable: the AWS SDK default credential chain supplies them to the server process, and the region is set on each DSQL connection profile.

### Security

Local single-user mode is the default and has no login gate. Do not expose it to untrusted networks: bind it to loopback for local use, or firewall it to trusted clients. For a shared deployment, set `PGPAINT_AUTH=1`, use a strong `SECRET_KEY_BASE`, configure `AUTH_ORIGIN` to match the externally visible HTTPS origin when using OAuth or passkeys, terminate TLS in front of pgpaint, and restrict network ingress to trusted users. Shared mode uses Gluon's default registration policy; it does not provide a pgpaint-specific invite allowlist. Keep `GLUON_INSECURE_COOKIE` unset in production. The authentication support is provided by the Gluon framework, which is under development and is not security-certified; assess it against your deployment requirements.

Accounts can create connection profiles, so pgpaint opens outbound database connections on their behalf. Deploy shared servers only on networks trusted by those users, and apply appropriate egress controls.

## Development

For front-end work, build once or keep Bun's asset watcher running:

```sh
cd web
bun install --frozen-lockfile
bun run build
bun run watch
```

In another terminal, run the app with `GLUON_DEV_TEMPLATES=1` so it reads the latest TSX and public assets from disk. The default compiled runtime uses embedded files.

The Testcontainers-based test suite requires a reachable Docker daemon:

```sh
cargo test --workspace --all-targets
```

`PGPAINT_TESTCONTAINERS_HOST` is only needed when the test process cannot reach mapped container ports using the host reported by Docker. It changes the connection host used for PostgreSQL, CockroachDB, YugabyteDB, and Floci's control plane; Testcontainers still uses `DOCKER_HOST` to reach the Docker API. DSQL keeps Floci's TLS/SNI hostname and uses a local TCP forwarder to reach the override host.

Start the local development stack with seeded metadata PostgreSQL and all four supported engines:

```sh
cargo run -p pgpaint-testkit --example devstack
```

The devstack prints connection details and runs until Ctrl-C. Its DSQL emulator is `floci/floci:nightly-10292026`: Floci 2.3.0 rejects the DSQL `CreateCluster` service scope, so the testkit pins a nightly until stable support is released. Floci also needs access to the Docker socket mounted inside its container. First-run image pulls are large; YugabyteDB and DSQL can take several minutes to become ready.

