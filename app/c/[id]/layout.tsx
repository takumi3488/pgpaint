import CsrfToken from "../../components/csrf_token.tsx";
import EngineBadge from "../../components/engine_badge.tsx";
import Tabs from "../../components/tabs.tsx";

export default function WorkspaceLayout(props) {
  return (
    <section class="flex min-h-[calc(100vh-4rem)] flex-col">
      <header class="flex flex-wrap items-center justify-between gap-4 border-b border-slate-200 bg-white px-5 py-4 dark:border-slate-800 dark:bg-slate-950">
        <div class="flex min-w-0 flex-wrap items-center gap-3">
          <h1 class="truncate text-lg font-semibold">{props.profile_name}</h1>
          <EngineBadge engine={props.engine_label} />
          <span class="text-sm text-slate-500 dark:text-slate-400">{props.server_version}</span>
          {props.read_only && <span class="rounded-full border border-amber-300 bg-amber-50 px-2.5 py-1 text-xs font-semibold text-amber-800 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-200">Read only</span>}
        </div>
        {props.connected && (
          <form method="post" action={props.disconnect_url}>
            <CsrfToken token={props.csrf_token} />
            <button class="rounded-md border border-slate-300 px-3 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900" type="submit">Disconnect</button>
          </form>
        )}
      </header>
      <div class="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[17rem_minmax(0,1fr)]">
        <aside class="border-b border-slate-200 bg-slate-50 p-4 dark:border-slate-800 dark:bg-slate-900 md:border-b-0 md:border-r">
          <h2 class="mb-3 text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">Schema</h2>
          <div class="min-h-28 rounded-md border border-dashed border-slate-300 p-3 text-sm text-slate-500 dark:border-slate-700 dark:text-slate-400" hx-get={props.tree_url} hx-trigger="load" hx-target="this" hx-swap="innerHTML">
            <p>Schema browser will appear here when connected.</p>
          </div>
        </aside>
        <div class="min-w-0">
          <Tabs tabs={props.tabs} />
          <main id="workspace-content" class="min-w-0 p-5">{props.children}</main>
        </div>
      </div>
    </section>
  );
}
