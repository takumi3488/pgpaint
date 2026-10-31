import CsrfToken from "../../components/csrf_token.tsx";

export default function EditSavedQueryPage(props) {
  return (
    <main class="mx-auto max-w-4xl px-5 py-10">
      <a class="text-sm font-medium text-sky-700 hover:underline dark:text-sky-300" href="/saved">← Saved queries</a>
      <header class="mb-7 mt-5">
        <p class="text-sm font-semibold uppercase tracking-widest text-sky-700 dark:text-sky-300">Query library</p>
        <h1 class="mt-2 text-3xl font-semibold tracking-tight">Edit saved query</h1>
      </header>
      <form class="space-y-5 rounded-xl border border-slate-200 bg-white p-5 dark:border-slate-800 dark:bg-slate-900" method="post" action={"/saved/" + props.id}>
        <CsrfToken token={props.csrf_token} />
        <input type="hidden" name="operation" value="save" />
        <label class="block text-sm font-medium">
          Name
          <input class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" type="text" name="name" value={props.name} required />
        </label>
        <label class="block text-sm font-medium">
          Default profile <span class="font-normal text-slate-500">(optional)</span>
          <select class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" name="profile_id">
            <option value="" selected={props.no_profile_selected}>No default profile</option>
            {props.profiles.map((profile) => <option value={profile.id} selected={profile.selected}>{profile.name}</option>)}
          </select>
        </label>
        <label class="block text-sm font-medium">
          SQL
          <textarea class="min-h-72 w-full rounded-md border border-slate-300 bg-white px-3 py-2 font-mono text-sm dark:border-slate-700 dark:bg-slate-950" name="sql" required>{props.sql}</textarea>
        </label>
        <div class="flex flex-wrap items-center gap-3">
          <button class="rounded-md bg-sky-700 px-4 py-2 text-sm font-semibold text-white hover:bg-sky-600" type="submit">Save changes</button>
          {props.open_href && <a class="rounded-md border border-slate-300 px-4 py-2 text-sm font-semibold hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-950" href={props.open_href}>Open in editor</a>}
          <a class="text-sm font-medium text-slate-600 hover:underline dark:text-slate-300" href="/saved">Cancel</a>
        </div>
      </form>
      {!props.open_href && (
        <form class="mt-5 flex flex-wrap items-end gap-3 rounded-xl border border-slate-200 p-5 dark:border-slate-800" method="post" action={"/saved/" + props.id}>
          <CsrfToken token={props.csrf_token} />
          <input type="hidden" name="operation" value="open" />
          <label class="block min-w-56 text-sm font-medium">
            Choose a profile to open this query
            <select class="mt-1.5 w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" name="profile_id" required>
              <option value="">Select profile</option>
              {props.profiles.map((profile) => <option value={profile.id}>{profile.name}</option>)}
            </select>
          </label>
          <button class="rounded-md bg-sky-700 px-4 py-2 text-sm font-semibold text-white hover:bg-sky-600" type="submit" disabled={!props.has_profiles}>Open in editor</button>
          {!props.has_profiles && <p class="text-sm text-slate-500 dark:text-slate-400">Create a connection profile before opening this query.</p>}
        </form>
      )}
      <form class="mt-5" method="post" action={"/saved/" + props.id} hx-post={"/saved/" + props.id} hx-confirm="Delete this saved query? This cannot be undone.">
        <CsrfToken token={props.csrf_token} />
        <input type="hidden" name="operation" value="delete" />
        <button class="text-sm font-medium text-rose-700 hover:underline dark:text-rose-300" type="submit">Delete saved query</button>
      </form>
    </main>
  );
}
