import EngineViewResults from "./results.tsx";

export default function EngineView(props: {
  view_title: string;
  relation_title: string | null;
  relations: Array<{ label: string; href: string }>;
  relation_picker: boolean;
  relations_empty: boolean;
  show_refresh: boolean;
  result_loaded: boolean;
  error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null;
  columns: Array<{ label: string; type_name: string }>;
  rows: Array<{ cells: Array<{ value: string; is_null: boolean }> }>;
  truncated: boolean;
  is_activity: boolean;
  refresh_url: string;
}) {
  return (
    <main id="view-page" class="space-y-5">
      <header>
        <h1 class="text-xl font-semibold text-slate-900 dark:text-slate-100">{props.view_title}</h1>
        {props.relation_title && <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">{props.relation_title}</p>}
      </header>
      {props.relation_picker && (
        <section class="space-y-3 rounded-lg border border-slate-200 bg-white p-4 dark:border-slate-800 dark:bg-slate-950">
          <h2 class="font-medium text-slate-900 dark:text-slate-100">Choose a relation</h2>
          <p class="text-sm text-slate-600 dark:text-slate-300">This engine view needs a table or view. Select one to continue.</p>
          {props.relations_empty && <p class="text-sm text-slate-500 dark:text-slate-400">No relations are available in this database.</p>}
          <ul class="divide-y divide-slate-100 dark:divide-slate-800">
            {props.relations.map((relation) => <li><a class="block py-2 text-sm text-sky-700 hover:underline dark:text-sky-300" href={relation.href}>{relation.label}</a></li>)}
          </ul>
        </section>
      )}
      {props.show_refresh && (
        <section class="space-y-3">
          <div class="flex flex-wrap items-center gap-4">
            <button class="rounded-md border border-slate-300 bg-white px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200 dark:hover:bg-slate-800" type="button" hx-get={props.refresh_url} hx-target="#view-page" hx-swap="outerHTML">Refresh</button>
            {props.is_activity && <label class="inline-flex items-center gap-2 text-sm text-slate-600 dark:text-slate-300"><input id="view-auto-refresh" type="checkbox" checked hx-preserve class="rounded border-slate-300 text-sky-600 focus:ring-sky-500 dark:border-slate-700" />Auto-refresh every 5 seconds</label>}
          </div>
          <EngineViewResults result_loaded={props.result_loaded} error={props.error} columns={props.columns} rows={props.rows} truncated={props.truncated} is_activity={props.is_activity} refresh_url={props.refresh_url} />
        </section>
      )}
    </main>
  );
}
