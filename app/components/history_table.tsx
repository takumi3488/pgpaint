import CsrfToken from "./csrf_token.tsx";

export default function HistoryTable(props: {
  entries: Array<{
    id: string;
    profile_name: string;
    executed_at: string;
    elapsed: string;
    rows: string;
    status_label: string;
    sql_preview: string;
    sql: string;
    open_href: string;
  }>;
  action_url: string;
  csrf_token: string;
  profile_id: string;
  search: string;
  status: string;
  page: number;
  total_pages: number;
  has_entries: boolean;
  previous_page?: number | null;
  next_page?: number | null;
}) {
  return (
    <section class="mt-6">
      {props.has_entries ? (
        <div class="overflow-x-auto rounded-xl border border-stone-200 bg-white dark:border-stone-800 dark:bg-stone-900">
          <table class="w-full min-w-[58rem] border-collapse text-left text-sm">
            <thead class="bg-stone-50 text-xs uppercase tracking-wide text-stone-500 dark:bg-stone-950 dark:text-stone-400">
              <tr>
                <th scope="col" class="px-4 py-3">Time</th>
                <th scope="col" class="px-4 py-3">Profile</th>
                <th scope="col" class="px-4 py-3">Status</th>
                <th scope="col" class="px-4 py-3">Elapsed</th>
                <th scope="col" class="px-4 py-3">Rows</th>
                <th scope="col" class="px-4 py-3">SQL</th>
                <th scope="col" class="px-4 py-3">Actions</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-stone-200 dark:divide-stone-800">
              {props.entries.map((entry) => (
                <tr class="align-top">
                  <td class="whitespace-nowrap px-4 py-4 text-stone-600 dark:text-stone-300"><time>{entry.executed_at}</time></td>
                  <td class="max-w-48 px-4 py-4 font-medium">{entry.profile_name}</td>
                  <td class="px-4 py-4"><span class="inline-flex rounded-full border border-stone-300 bg-stone-100 px-2.5 py-1 text-xs font-semibold text-stone-700 dark:border-stone-700 dark:bg-stone-800 dark:text-stone-200">{entry.status_label}</span></td>
                  <td class="whitespace-nowrap px-4 py-4">{entry.elapsed}</td>
                  <td class="whitespace-nowrap px-4 py-4">{entry.rows}</td>
                  <td class="max-w-xl px-4 py-4">
                    <pre class="max-h-24 overflow-hidden whitespace-pre-wrap break-all font-mono text-xs text-stone-700 dark:text-stone-300">{entry.sql_preview}</pre>
                    <details class="mt-2">
                      <summary class="cursor-pointer text-xs font-medium text-orange-700 hover:text-orange-800 dark:text-orange-300">Show full SQL</summary>
                      <pre class="mt-2 max-h-96 overflow-auto whitespace-pre-wrap break-all rounded-md bg-stone-50 p-3 font-mono text-xs dark:bg-stone-950">{entry.sql}</pre>
                    </details>
                  </td>
                  <td class="px-4 py-4">
                    <div class="flex flex-col items-start gap-2">
                      <a class="text-sm font-medium text-orange-700 hover:underline dark:text-orange-300" href={entry.open_href} aria-label={"Open query from " + entry.profile_name + " in editor"}>Open in editor</a>
                      <form method="post" action={props.action_url} hx-post={props.action_url} hx-confirm="Delete this history entry?">
                        <CsrfToken token={props.csrf_token} />
                        <input type="hidden" name="operation" value="delete" />
                        <input type="hidden" name="history_id" value={entry.id} />
                        <input type="hidden" name="profile_id" value={props.profile_id} />
                        <input type="hidden" name="search" value={props.search} />
                        <input type="hidden" name="status" value={props.status} />
                        <input type="hidden" name="page" value={props.page} />
                        <button class="text-sm font-medium text-rose-700 hover:underline dark:text-rose-300" type="submit" aria-label={"Delete history entry from " + entry.executed_at}>Delete</button>
                      </form>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <div class="rounded-xl border border-dashed border-stone-300 px-6 py-12 text-center dark:border-stone-700">
          <h2 class="text-lg font-semibold">No query history found</h2>
          <p class="mt-2 text-sm text-stone-500 dark:text-stone-400">Run a query to see it appear here.</p>
        </div>
      )}
      <nav class="mt-5 flex items-center justify-between text-sm" aria-label="History pagination">
        <div>
          {props.previous_page && (
            <form method="get" action={props.action_url}>
              <input type="hidden" name="profile_id" value={props.profile_id} />
              <input type="hidden" name="search" value={props.search} />
              <input type="hidden" name="status" value={props.status} />
              <input type="hidden" name="page" value={props.previous_page} />
              <button class="rounded-md border border-stone-300 px-3 py-2 hover:bg-stone-50 dark:border-stone-700 dark:hover:bg-stone-900" type="submit">Previous</button>
            </form>
          )}
        </div>
        <span class="text-stone-600 dark:text-stone-400">Page {props.page} of {props.total_pages}</span>
        <div>
          {props.next_page && (
            <form method="get" action={props.action_url}>
              <input type="hidden" name="profile_id" value={props.profile_id} />
              <input type="hidden" name="search" value={props.search} />
              <input type="hidden" name="status" value={props.status} />
              <input type="hidden" name="page" value={props.next_page} />
              <button class="rounded-md border border-stone-300 px-3 py-2 hover:bg-stone-50 dark:border-stone-700 dark:hover:bg-stone-900" type="submit">Next</button>
            </form>
          )}
        </div>
      </nav>
    </section>
  );
}
