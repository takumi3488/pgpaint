import CsrfToken from "../../components/csrf_token.tsx";
import DbError from "../../components/db_error.tsx";

export default function ConnectPage(props) {
  return (
    <section class="mx-auto max-w-2xl">
      <h2 class="text-2xl font-semibold">Connect to {props.profile_name}</h2>
      <p class="mt-2 text-sm text-slate-600 dark:text-slate-400">{props.engine_label} · {props.profile_name}</p>
      <div class="mt-6"><DbError error={props.connect_error} /></div>
      {props.form_error && <p class="mt-4 rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100" role="alert">{props.form_error}</p>}
      <form class="mt-6 space-y-5 rounded-xl border border-slate-200 bg-white p-6 dark:border-slate-800 dark:bg-slate-900" method="post" action={props.connect_url}>
        <CsrfToken token={props.csrf_token} />
        {props.password_required && !props.has_saved_password && (
          <label class="block space-y-2 text-sm font-medium">
            <span>Password</span>
            <input class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-slate-950 outline-none focus:border-sky-600 focus:ring-2 focus:ring-sky-200 dark:border-slate-700 dark:bg-slate-950 dark:text-white" type="password" name="password" autocomplete="current-password" />
          </label>
        )}
        {!props.password_required && <p class="text-sm text-slate-600 dark:text-slate-400">Aurora DSQL uses IAM authentication; no password is required.</p>}
        <button class="rounded-md bg-sky-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-sky-600" type="submit">Connect</button>
      </form>
    </section>
  );
}
