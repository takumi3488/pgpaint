import CsrfToken from "../components/csrf_token.tsx";

export default function ConnectionForm(props: {
  action: string;
  csrf_token: string;
  is_edit: boolean;
  has_secret_cipher: boolean;
  values: {
    name: string;
    postgres_selected: boolean;
    dsql_selected: boolean;
    cockroach_selected: boolean;
    yugabyte_selected: boolean;
    host: string;
    port: string;
    database: string;
    username: string;
    ssl_disable_selected: boolean;
    ssl_prefer_selected: boolean;
    ssl_require_selected: boolean;
    ssl_verify_ca_selected: boolean;
    ssl_verify_full_selected: boolean;
    ssl_root_cert_pem: string;
    read_only: boolean;
    statement_timeout_ms: string;
    color: string;
    iam_region: string;
    iam_admin: boolean;
    source_profile_id: string;
    has_saved_password: boolean;
    clear_color: boolean;
    save_password: boolean;
    clear_saved_password: boolean;
  };
  errors: {
    name?: string | null;
    engine?: string | null;
    host?: string | null;
    port?: string | null;
    database?: string | null;
    username?: string | null;
    password?: string | null;
    ssl_mode?: string | null;
    ssl_root_cert_pem?: string | null;
    read_only?: string | null;
    statement_timeout_ms?: string | null;
    color?: string | null;
    iam_region?: string | null;
    general?: string | null;
  };
}) {
  return (
    <form id="connection-profile-form" method="post" action={props.action} data-connection-form class="space-y-8">
      <CsrfToken token={props.csrf_token} />
      <input type="hidden" name="source_profile_id" value={props.values.source_profile_id} />
      {props.errors.general && <p class="rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-800 dark:border-red-900 dark:bg-red-950 dark:text-red-100" role="alert">{props.errors.general}</p>}
      <section class="grid gap-5 rounded-xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900 sm:grid-cols-2">
        <div class="sm:col-span-2">
          <label for="connection-name" class="mb-1 block text-sm font-medium">Name</label>
          <input id="connection-name" name="name" value={props.values.name} required class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.name ? "true" : "false"} aria-describedby="connection-name-error" />
          {props.errors.name && <p id="connection-name-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.name}</p>}
        </div>
        <div>
          <label for="connection-engine" class="mb-1 block text-sm font-medium">Engine</label>
          <select id="connection-engine" name="engine" data-engine-select class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.engine ? "true" : "false"} aria-describedby="connection-engine-error">
            <option value="postgres" data-default-port="5432" selected={props.values.postgres_selected}>PostgreSQL</option>
            <option value="aurora_dsql" data-default-port="5432" selected={props.values.dsql_selected}>Aurora DSQL</option>
            <option value="cockroach_db" data-default-port="26257" selected={props.values.cockroach_selected}>CockroachDB</option>
            <option value="yugabyte_db" data-default-port="5433" selected={props.values.yugabyte_selected}>YugabyteDB</option>
          </select>
          {props.errors.engine && <p id="connection-engine-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.engine}</p>}
        </div>
        <div>
          <label for="connection-host" class="mb-1 block text-sm font-medium">Host</label>
          <input id="connection-host" name="host" value={props.values.host} required autocomplete="url" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.host ? "true" : "false"} aria-describedby="connection-host-error" />
          {props.errors.host && <p id="connection-host-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.host}</p>}
        </div>
        <div>
          <label for="connection-port" class="mb-1 block text-sm font-medium">Port</label>
          <input id="connection-port" name="port" type="number" min="1" max="65535" step="1" value={props.values.port} required class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.port ? "true" : "false"} aria-describedby="connection-port-error" />
          {props.errors.port && <p id="connection-port-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.port}</p>}
        </div>
        <div>
          <label for="connection-database" class="mb-1 block text-sm font-medium">Database</label>
          <input id="connection-database" name="database" value={props.values.database} class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.database ? "true" : "false"} aria-describedby="connection-database-error" />
          {props.errors.database && <p id="connection-database-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.database}</p>}
        </div>
        <div>
          <label for="connection-username" class="mb-1 block text-sm font-medium">Username</label>
          <input id="connection-username" name="username" value={props.values.username} autocomplete="username" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.username ? "true" : "false"} aria-describedby="connection-username-error" />
          {props.errors.username && <p id="connection-username-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.username}</p>}
        </div>
        <fieldset data-password-fields hidden={props.values.dsql_selected} disabled={props.values.dsql_selected} class="min-w-0 border-0 p-0">
          <legend class="text-sm font-medium">Password authentication</legend>
          <label for="connection-password" class="mb-1 block text-sm font-medium">Password</label>
          <input id="connection-password" name="password" type="password" autocomplete="new-password" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.password ? "true" : "false"} aria-describedby="connection-password-error" />
          {props.errors.password && <p id="connection-password-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.password}</p>}
          <label class="mt-3 flex items-center gap-2 text-sm">
            <input name="save_password" type="checkbox" value="on" checked={props.values.save_password} disabled={!props.has_secret_cipher} class="rounded border-slate-300 text-sky-700 focus:ring-sky-600" />
            Save password securely
          </label>
          {!props.has_secret_cipher && <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">Password saving is disabled because SECRET_KEY_BASE is not configured.</p>}
          {props.values.has_saved_password && <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">A password is already saved. Leave this blank to keep it, or replace or clear it below.</p>}
          {props.values.has_saved_password && <label class="mt-2 flex items-center gap-2 text-sm"><input name="clear_saved_password" type="checkbox" value="on" checked={props.values.clear_saved_password} class="rounded border-slate-300 text-sky-700 focus:ring-sky-600" />Clear saved password</label>}
        </fieldset>
        <div data-dsql-fields hidden={!props.values.dsql_selected} class="space-y-4">
          <div>
            <label for="connection-iam-region" class="mb-1 block text-sm font-medium">IAM region</label>
            <input id="connection-iam-region" name="iam_region" value={props.values.iam_region} autocomplete="off" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.iam_region ? "true" : "false"} aria-describedby="connection-iam-region-error" />
            {props.errors.iam_region && <p id="connection-iam-region-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.iam_region}</p>}
          </div>
          <label class="flex items-center gap-2 text-sm"><input name="iam_admin" type="checkbox" value="on" checked={props.values.iam_admin} class="rounded border-slate-300 text-sky-700 focus:ring-sky-600" />Use the DSQL admin role</label>
        </div>
        <div>
          <label for="connection-ssl-mode" class="mb-1 block text-sm font-medium">SSL mode</label>
          <select id="connection-ssl-mode" name="ssl_mode" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.ssl_mode ? "true" : "false"} aria-describedby="connection-ssl-mode-error">
            <option value="disable" selected={props.values.ssl_disable_selected}>Disable</option>
            <option value="prefer" selected={props.values.ssl_prefer_selected}>Prefer</option>
            <option value="require" selected={props.values.ssl_require_selected}>Require</option>
            <option value="verify-ca" selected={props.values.ssl_verify_ca_selected}>Verify CA</option>
            <option value="verify-full" selected={props.values.ssl_verify_full_selected}>Verify full</option>
          </select>
          {props.errors.ssl_mode && <p id="connection-ssl-mode-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.ssl_mode}</p>}
        </div>
        <div>
          <label for="connection-root-cert" class="mb-1 block text-sm font-medium">Root certificate PEM</label>
          <textarea id="connection-root-cert" name="ssl_root_cert_pem" rows="4" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 font-mono text-xs dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.ssl_root_cert_pem ? "true" : "false"} aria-describedby="connection-root-cert-error">{props.values.ssl_root_cert_pem}</textarea>
          {props.errors.ssl_root_cert_pem && <p id="connection-root-cert-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.ssl_root_cert_pem}</p>}
        </div>
        <div>
          <label for="connection-timeout" class="mb-1 block text-sm font-medium">Statement timeout (ms)</label>
          <input id="connection-timeout" name="statement_timeout_ms" type="number" min="0" step="1" value={props.values.statement_timeout_ms} class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.statement_timeout_ms ? "true" : "false"} aria-describedby="connection-timeout-error" />
          {props.errors.statement_timeout_ms && <p id="connection-timeout-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.statement_timeout_ms}</p>}
        </div>
        <div>
          <label for="connection-color" class="mb-1 block text-sm font-medium">Color tag</label>
          <input id="connection-color" name="color" type="color" value={props.values.color} class="h-10 w-16 rounded-md border border-slate-300 bg-white p-1 dark:border-slate-700 dark:bg-slate-950" aria-invalid={props.errors.color ? "true" : "false"} aria-describedby="connection-color-error" />
          {props.errors.color && <p id="connection-color-error" class="mt-1 text-sm text-red-700 dark:text-red-300" role="alert">{props.errors.color}</p>}
          <label class="mt-2 flex items-center gap-2 text-sm"><input name="clear_color" type="checkbox" value="on" checked={props.values.clear_color} class="rounded border-slate-300 text-sky-700 focus:ring-sky-600" />No color tag</label>
        </div>
        <label class="flex items-center gap-2 text-sm"><input name="read_only" type="checkbox" value="on" checked={props.values.read_only} class="rounded border-slate-300 text-sky-700 focus:ring-sky-600" />Open in read-only mode</label>
      </section>
      <div class="flex flex-wrap items-center gap-3">
        <button type="submit" formaction="/connections/test" formmethod="post" hx-post="/connections/test" hx-include="closest form" hx-target="#connection-test-result" hx-swap="innerHTML" class="rounded-md border border-slate-300 px-4 py-2.5 text-sm font-semibold hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900">Test connection</button>
        <button type="submit" class="rounded-md bg-sky-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-sky-600">{props.is_edit ? "Save changes" : "Create connection"}</button>
      </div>
      <div id="connection-test-result" aria-live="polite"></div>
    </form>
  );
}
