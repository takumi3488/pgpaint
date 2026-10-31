import ConnectionForm from "../../connection_form.tsx";
import CsrfToken from "../../../components/csrf_token.tsx";

export default function EditConnectionPage(props) {
  return (
    <main class="mx-auto max-w-4xl px-5 py-10">
      <a href="/" class="text-sm font-medium text-sky-700 hover:underline dark:text-sky-300">← Back to connections</a>
      <h1 class="mt-4 text-3xl font-semibold tracking-tight">Edit connection</h1>
      <p class="mt-2 text-slate-600 dark:text-slate-400">Update the profile settings and test the connection before saving.</p>
      <div class="mt-8">
        <ConnectionForm action={props.action} csrf_token={props.csrf_token} is_edit={props.is_edit} has_secret_cipher={props.has_secret_cipher} values={props.values} errors={props.errors} />
      </div>
      <section class="mt-12 border-t border-slate-200 pt-8 dark:border-slate-800">
        <h2 class="text-lg font-semibold">Delete connection</h2>
        <p class="mt-1 text-sm text-slate-600 dark:text-slate-400">Deleting a profile also closes its active workspace connection.</p>
        <form method="post" action={props.action} data-confirm="Delete this connection profile?" class="mt-4">
          <CsrfToken token={props.csrf_token} />
          <input type="hidden" name="_action" value="delete" />
          <button type="submit" class="rounded-md border border-red-300 px-4 py-2 text-sm font-semibold text-red-800 hover:bg-red-50 dark:border-red-900 dark:text-red-200 dark:hover:bg-red-950">Delete profile</button>
        </form>
      </section>
    </main>
  );
}
