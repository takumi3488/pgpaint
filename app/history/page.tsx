import HistoryTable from "../components/history_table.tsx";
import CsrfToken from "../components/csrf_token.tsx";

export default function HistoryPage(props) {
  return (
    <main class="mx-auto max-w-screen-2xl px-5 py-10">
      <header class="mb-7">
        <p class="text-sm font-semibold uppercase tracking-widest text-orange-700 dark:text-orange-300">Activity</p>
        <h1 class="mt-2 text-3xl font-semibold tracking-tight">Query history</h1>
        <p class="mt-2 text-sm text-stone-600 dark:text-stone-400">Browse queries run from any connection profile.</p>
      </header>
      <section class="rounded-xl border border-stone-200 bg-white p-4 dark:border-stone-800 dark:bg-stone-900">
        <form class="grid gap-4 md:grid-cols-[minmax(14rem,1fr)_minmax(12rem,0.7fr)_minmax(12rem,0.7fr)_auto] md:items-end" method="get" action="/history">
          <label class="block text-sm font-medium">
            Search SQL
            <input class="mt-1.5 w-full rounded-md border border-stone-300 bg-white px-3 py-2 text-sm dark:border-stone-700 dark:bg-stone-950" type="search" name="search" value={props.search} placeholder="Search query text" />
          </label>
          <label class="block text-sm font-medium">
            Profile
            <select class="mt-1.5 w-full rounded-md border border-stone-300 bg-white px-3 py-2 text-sm dark:border-stone-700 dark:bg-stone-950" name="profile_id">
              <option value="" selected={props.all_profiles_selected}>All profiles</option>
              {props.profiles.map((profile) => <option value={profile.id} selected={profile.selected}>{profile.name}</option>)}
            </select>
          </label>
          <label class="block text-sm font-medium">
            Status
            <select class="mt-1.5 w-full rounded-md border border-stone-300 bg-white px-3 py-2 text-sm dark:border-stone-700 dark:bg-stone-950" name="status">
              <option value="" selected={props.status_all_selected}>All statuses</option>
              <option value="succeeded" selected={props.status_succeeded_selected}>Succeeded</option>
              <option value="failed" selected={props.status_failed_selected}>Failed</option>
              <option value="canceled" selected={props.status_canceled_selected}>Canceled</option>
            </select>
          </label>
          <button class="rounded-md bg-orange-700 px-4 py-2 text-sm font-semibold text-white hover:bg-orange-800" type="submit">Apply filters</button>
        </form>
        {props.profile_id && (
          <form class="mt-4 border-t border-stone-200 pt-4 dark:border-stone-800" method="post" action="/history" hx-post="/history" hx-confirm="Clear all query history for this profile? This cannot be undone.">
            <CsrfToken token={props.csrf_token} />
            <input type="hidden" name="operation" value="clear-profile" />
            <input type="hidden" name="profile_id" value={props.profile_id} />
            <input type="hidden" name="search" value={props.search} />
            <input type="hidden" name="status" value={props.status} />
            <input type="hidden" name="page" value={props.page} />
            <button class="text-sm font-medium text-rose-700 hover:underline dark:text-rose-300" type="submit">Clear all history for {props.selected_profile_name}</button>
          </form>
        )}
      </section>
      <HistoryTable
        entries={props.entries}
        action_url="/history"
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
    </main>
  );
}
