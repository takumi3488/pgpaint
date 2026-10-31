import HistoryTable from "../../../components/history_table.tsx";
import CsrfToken from "../../../components/csrf_token.tsx";

export default function WorkspaceHistoryPage(props) {
  return (
    <section>
      <header class="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <p class="text-sm font-semibold uppercase tracking-widest text-sky-700 dark:text-sky-300">Workspace</p>
          <h2 class="mt-2 text-2xl font-semibold tracking-tight">Query history</h2>
          <p class="mt-2 text-sm text-slate-600 dark:text-slate-400">Queries run with {props.profile_name}.</p>
        </div>
        <form method="post" action={props.action_url} hx-post={props.action_url} hx-confirm="Clear all query history for this profile? This cannot be undone.">
          <CsrfToken token={props.csrf_token} />
          <input type="hidden" name="operation" value="clear-profile" />
          <input type="hidden" name="search" value={props.search} />
          <input type="hidden" name="status" value={props.status} />
          <input type="hidden" name="page" value={props.page} />
          <button class="rounded-md border border-rose-300 px-3 py-2 text-sm font-medium text-rose-700 hover:bg-rose-50 dark:border-rose-900 dark:text-rose-300 dark:hover:bg-rose-950" type="submit">Clear profile history</button>
        </form>
      </header>
      <form class="grid gap-4 rounded-xl border border-slate-200 bg-white p-4 md:grid-cols-[minmax(14rem,1fr)_minmax(12rem,0.7fr)_auto] md:items-end dark:border-slate-800 dark:bg-slate-900" method="get" action={props.action_url}>
        <label class="block text-sm font-medium">
          Search SQL
          <input class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" type="search" name="search" value={props.search} placeholder="Search query text" />
        </label>
        <label class="block text-sm font-medium">
          Status
          <select class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" name="status">
            <option value="" selected={props.status_all_selected}>All statuses</option>
            <option value="succeeded" selected={props.status_succeeded_selected}>Succeeded</option>
            <option value="failed" selected={props.status_failed_selected}>Failed</option>
            <option value="canceled" selected={props.status_canceled_selected}>Canceled</option>
          </select>
        </label>
        <button class="rounded-md bg-sky-700 px-4 py-2 text-sm font-semibold text-white hover:bg-sky-600" type="submit">Apply filters</button>
      </form>
      <HistoryTable
        entries={props.entries}
        action_url={props.action_url}
        csrf_token={props.csrf_token}
        profile_id={props.profile_id}
        search={props.search}
        status={props.status}
        page={props.page}
        total_pages={props.total_pages}
        previous_page={props.previous_page}
        next_page={props.next_page}
        has_entries={props.has_entries}
      />
    </section>
  );
}
