import CsrfToken from "../components/csrf_token.tsx";

export default function SavedQueriesPage(props) {
  return (
    <main class="mx-auto max-w-screen-2xl px-5 py-10">
      <header class="mb-7">
        <p class="text-sm font-semibold uppercase tracking-widest text-sky-700 dark:text-sky-300">Library</p>
        <h1 class="mt-2 text-3xl font-semibold tracking-tight">Saved queries</h1>
        <p class="mt-2 text-sm text-slate-600 dark:text-slate-400">Keep reusable SQL close to the connection profile where it belongs.</p>
      </header>
      <section class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
        <h2 class="text-lg font-semibold">Save a query</h2>
        <form class="mt-4 grid gap-4" method="post" action="/saved">
          <CsrfToken token={props.csrf_token} />
          <input type="hidden" name="operation" value="create" />
          <label class="block text-sm font-medium">
            Name
            <input class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" type="text" name="name" required />
          </label>
          <label class="block text-sm font-medium">
            Default profile <span class="font-normal text-slate-500">(optional)</span>
            <select class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" name="profile_id">
              <option value="">No default profile</option>
              {props.profiles.map((profile) => <option value={profile.id}>{profile.name}</option>)}
            </select>
          </label>
          <label class="block text-sm font-medium">
            SQL
            <textarea class="mt-1.5 min-h-36 w-full rounded-md border border-slate-300 bg-white px-3 py-2 font-mono text-sm dark:border-slate-700 dark:bg-slate-950" name="sql" required></textarea>
          </label>
          <div><button class="rounded-md bg-sky-700 px-4 py-2 text-sm font-semibold text-white hover:bg-sky-600" type="submit">Save query</button></div>
        </form>
      </section>
      <section class="mt-8">
        <div class="mb-4 flex flex-wrap items-end justify-between gap-4">
          <div>
            <h2 class="text-xl font-semibold">Your saved queries</h2>
            <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">Search query names and SQL text.</p>
          </div>
          <form class="flex w-full max-w-lg gap-2" method="get" action="/saved">
            <label class="sr-only" for="saved-search">Search saved queries</label>
            <input id="saved-search" class="min-w-0 flex-1 rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" type="search" name="search" value={props.search} placeholder="Search name or SQL" />
            <button class="rounded-md border border-slate-300 px-4 py-2 text-sm font-semibold hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900" type="submit">Search</button>
          </form>
        </div>
        {props.has_entries ? (
          <div class="space-y-4">
            {props.entries.map((entry) => (
              <article class="rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900">
                <header class="flex flex-wrap items-start justify-between gap-3">
                  <div class="min-w-0">
                    <h3 class="truncate text-lg font-semibold">{entry.name}</h3>
                    <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">{entry.profile_name}</p>
                  </div>
                  <div class="flex flex-wrap items-center gap-4 text-sm">
                    <a class="font-medium text-sky-700 hover:underline dark:text-sky-300" href={entry.edit_href} aria-label={"Edit " + entry.name}>Edit</a>
                    <form method="post" action="/saved" hx-post="/saved" hx-confirm="Delete this saved query? This cannot be undone.">
                      <CsrfToken token={props.csrf_token} />
                      <input type="hidden" name="operation" value="delete" />
                      <input type="hidden" name="saved_id" value={entry.id} />
                      <input type="hidden" name="search" value={props.search} />
                      <button class="font-medium text-rose-700 hover:underline dark:text-rose-300" type="submit" aria-label={"Delete " + entry.name}>Delete</button>
                    </form>
                  </div>
                </header>
                <pre class="mt-4 max-h-24 overflow-hidden whitespace-pre-wrap break-all rounded-md bg-slate-50 p-3 font-mono text-xs dark:bg-slate-950">{entry.sql_preview}</pre>
                <div class="mt-4">
                  {entry.open_href ? (
                    <a class="inline-flex rounded-md bg-sky-700 px-3 py-2 text-sm font-semibold text-white hover:bg-sky-600" href={entry.open_href} aria-label={"Open " + entry.name + " in editor"}>Open in editor</a>
                  ) : (
                    <form class="flex flex-wrap items-end gap-3" method="post" action="/saved">
                      <CsrfToken token={props.csrf_token} />
                      <input type="hidden" name="operation" value="open" />
                      <input type="hidden" name="saved_id" value={entry.id} />
                      <input type="hidden" name="search" value={props.search} />
                      {props.has_profiles ? (
                        <>
                          <label class="block min-w-56 text-sm font-medium">
                            Choose a profile
                            <select class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" name="profile_id" required>
                              <option value="">Select profile</option>
                              {props.profiles.map((profile) => <option value={profile.id}>{profile.name}</option>)}
                            </select>
                          </label>
                          <button class="rounded-md bg-sky-700 px-3 py-2 text-sm font-semibold text-white hover:bg-sky-600" type="submit">Open in editor</button>
                        </>
                      ) : (
                        <p class="text-sm text-slate-500 dark:text-slate-400">Create a connection profile before opening this query.</p>
                      )}
                    </form>
                  )}
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div class="rounded-xl border border-dashed border-slate-300 px-6 py-12 text-center dark:border-slate-700">
            <h3 class="text-lg font-semibold">No saved queries found</h3>
            <p class="mt-2 text-sm text-slate-500 dark:text-slate-400">Save SQL above or adjust your search.</p>
          </div>
        )}
      </section>
    </main>
  );
}
