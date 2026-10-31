import CsrfToken from "./components/csrf_token.tsx";
import EngineBadge from "./components/engine_badge.tsx";

export default function HomePage(props: {
  csrf_token: string;
  connections: Array<{
    id: string;
    name: string;
    engine: string;
    host: string;
    port: number;
    database: string;
    read_only: boolean;
    color?: string | null;
    color_style: string;
    has_saved_password: boolean;
  }>;
  is_empty: boolean;
}) {
  return (
    <main class="mx-auto max-w-screen-2xl px-5 py-10">
      <div class="mb-8 flex flex-wrap items-end justify-between gap-4">
        <div>
          <p class="text-sm font-semibold uppercase tracking-widest text-sky-700 dark:text-sky-300">PostgreSQL workspace</p>
          <h1 class="mt-2 text-3xl font-semibold tracking-tight">Connections</h1>
          <p class="mt-2 text-slate-600 dark:text-slate-400">Manage database profiles and open a workspace.</p>
        </div>
        <a href="/connections/new" class="inline-flex rounded-md bg-sky-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-sky-600">Add connection</a>
      </div>
      {props.is_empty ? (
        <section class="rounded-2xl border border-slate-200 bg-white p-10 shadow-sm dark:border-slate-800 dark:bg-slate-900">
          <h2 class="text-xl font-semibold">No connections yet</h2>
          <p class="mt-2 text-slate-600 dark:text-slate-400">Create a profile to browse schemas, run queries, and inspect your database.</p>
          <a href="/connections/new" class="mt-6 inline-flex rounded-md bg-sky-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-sky-600">Create a connection</a>
        </section>
      ) : (
        <section class="grid gap-4 md:grid-cols-2 xl:grid-cols-3" aria-label="Saved connections">
          {props.connections.map((profile) => (
            <article class="flex flex-col rounded-xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <h2 class="text-lg font-semibold">{profile.name}</h2>
                  <p class="mt-1 break-all font-mono text-sm text-slate-600 dark:text-slate-400">{profile.host}:{profile.port}</p>
                </div>
                <EngineBadge engine={profile.engine} />
              </div>
              <p class="mt-3 text-sm text-slate-700 dark:text-slate-300">Database: {profile.database || "default"}</p>
              <div class="mt-3 flex flex-wrap items-center gap-2">
                {profile.color && <span class="h-3 w-3 rounded-full ring-1 ring-slate-400/50" style={profile.color_style} aria-label={"Color tag " + profile.color}></span>}
                {profile.read_only && <span class="rounded-full border border-amber-300 bg-amber-50 px-2 py-0.5 text-xs font-medium text-amber-900 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-100">Read-only</span>}
                {profile.has_saved_password && <span class="text-xs text-slate-500 dark:text-slate-400">Password saved</span>}
              </div>
              <div class="mt-auto flex flex-wrap items-center gap-2 pt-6">
                <a href={"/c/" + profile.id} class="rounded-md bg-sky-700 px-3 py-2 text-sm font-semibold text-white hover:bg-sky-600">Open workspace</a>
                <a href={"/connections/" + profile.id + "/edit"} class="rounded-md border border-slate-300 px-3 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800">Edit</a>
                <a href={"/connections/new?duplicate=" + profile.id} class="rounded-md border border-slate-300 px-3 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800">Duplicate</a>
                <form method="post" action={"/connections/" + profile.id + "/edit"} data-confirm={"Delete " + profile.name + "?"}>
                  <CsrfToken token={props.csrf_token} />
                  <input type="hidden" name="_action" value="delete" />
                  <button type="submit" class="rounded-md border border-red-300 px-3 py-2 text-sm font-medium text-red-800 hover:bg-red-50 dark:border-red-900 dark:text-red-200 dark:hover:bg-red-950">Delete</button>
                </form>
              </div>
            </article>
          ))}
        </section>
      )}
    </main>
  );
}
