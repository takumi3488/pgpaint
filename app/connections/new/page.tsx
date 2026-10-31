import ConnectionForm from "../connection_form.tsx";

export default function NewConnectionPage(props) {
  return (
    <main class="mx-auto max-w-4xl px-5 py-10">
      <a href="/" class="text-sm font-medium text-sky-700 hover:underline dark:text-sky-300">← Back to connections</a>
      <h1 class="mt-4 text-3xl font-semibold tracking-tight">New connection</h1>
      <p class="mt-2 text-slate-600 dark:text-slate-400">Configure a database profile. Test the connection before saving.</p>
      <div class="mt-8">
        <ConnectionForm action={props.action} csrf_token={props.csrf_token} is_edit={props.is_edit} has_secret_cipher={props.has_secret_cipher} values={props.values} errors={props.errors} />
      </div>
    </main>
  );
}
