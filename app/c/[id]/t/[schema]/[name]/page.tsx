import DbError from "../../../../../components/db_error.tsx";

export default function TablePage(props: { detail: null | { relation: { schema: string; name: string }; kind: string; columns: Array<{ name: string; data_type: string; nullable: boolean; default: string | null; generated: string | null; comment: string | null; hidden: boolean }>; primary_key: string[]; indexes: Array<{ name: string; definition: string; columns: string[]; unique: boolean; primary: boolean; status: string | null }>; constraints: Array<{ name: string; kind: string; definition: string }>; foreign_keys: Array<{ name: string; columns: string[]; referenced: { schema: string; name: string }; referenced_columns: string[]; on_update: string; on_delete: string }>; comment: string | null; ddl: string }; table_tabs: Array<{ label: string; href: string; active: boolean }>; rows_url: string; relation_views: Array<{ title: string; href: string }>; no_relation_views: boolean; data_active: boolean; structure_active: boolean; ddl_active: boolean; views_active: boolean; error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null }) {
  return (
    <section class="space-y-5">
      <div>
        <p class="text-sm text-slate-500 dark:text-slate-400">{props.detail && (props.detail.relation.schema + "." + props.detail.relation.name)}</p>
        {props.detail && <h2 class="mt-1 text-xl font-semibold text-slate-950 dark:text-white">{props.detail.relation.name}</h2>}
        {props.detail && props.detail.comment && <p class="mt-2 text-sm text-slate-600 dark:text-slate-300">{props.detail.comment}</p>}
      </div>
      <nav class="flex gap-2 border-b border-slate-200 dark:border-slate-800" aria-label="Table sections">
        {props.table_tabs.map((tab) => <a href={tab.href} aria-current={tab.active ? "page" : null} class={tab.active ? "border-b-2 border-sky-600 px-3 py-2 text-sm font-medium text-sky-700 dark:text-sky-300" : "border-b-2 border-transparent px-3 py-2 text-sm text-slate-600 hover:text-slate-950 dark:text-slate-400 dark:hover:text-white"}>{tab.label}</a>)}
      </nav>
      <DbError error={props.error} />
      {props.detail && props.data_active && (
        <div id="table-rows" hx-get={props.rows_url} hx-trigger="load" hx-target="this" hx-swap="innerHTML" class="min-w-0">
          <p class="text-sm text-slate-500 dark:text-slate-400">Loading rows…</p>
        </div>
      )}
      {props.detail && props.structure_active && (
        <div class="space-y-6">
          <section>
            <h3 class="mb-3 text-base font-semibold">Columns</h3>
            <div class="overflow-x-auto rounded-lg border border-slate-200 dark:border-slate-800">
              <table class="min-w-full divide-y divide-slate-200 text-sm dark:divide-slate-800">
                <thead class="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500 dark:bg-slate-900 dark:text-slate-400"><tr><th class="px-3 py-2">Name</th><th class="px-3 py-2">Type</th><th class="px-3 py-2">Nullable</th><th class="px-3 py-2">Default</th><th class="px-3 py-2">Generated</th><th class="px-3 py-2">Comment</th><th class="px-3 py-2">Visibility</th></tr></thead>
                <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
                  {props.detail.columns.map((column) => <tr><td class="whitespace-nowrap px-3 py-2 font-mono">{column.name}</td><td class="px-3 py-2 font-mono">{column.data_type}</td><td class="px-3 py-2">{column.nullable ? "Yes" : "No"}</td><td class="px-3 py-2 font-mono">{column.default || "—"}</td><td class="px-3 py-2 font-mono">{column.generated || "—"}</td><td class="px-3 py-2">{column.comment || "—"}</td><td class="px-3 py-2">{column.hidden ? <span class="text-amber-700 dark:text-amber-300">Hidden</span> : "Visible"}</td></tr>)}
                </tbody>
              </table>
            </div>
          </section>
          <section><h3 class="mb-2 text-base font-semibold">Primary key</h3><ul class="list-inside list-disc text-sm text-slate-700 dark:text-slate-300">{props.detail.primary_key.map((column) => <li class="font-mono">{column}</li>)}</ul></section>
          <section>
            <h3 class="mb-3 text-base font-semibold">Indexes</h3>
            <div class="overflow-x-auto rounded-lg border border-slate-200 dark:border-slate-800"><table class="min-w-full divide-y divide-slate-200 text-sm dark:divide-slate-800"><thead class="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500 dark:bg-slate-900 dark:text-slate-400"><tr><th class="px-3 py-2">Name</th><th class="px-3 py-2">Definition</th><th class="px-3 py-2">Status</th></tr></thead><tbody class="divide-y divide-slate-100 dark:divide-slate-800">{props.detail.indexes.map((index) => <tr><td class="px-3 py-2 font-mono">{index.name}</td><td class="px-3 py-2 font-mono">{index.definition}</td><td class="px-3 py-2">{index.status || "Ready"}</td></tr>)}</tbody></table></div>
          </section>
          <section><h3 class="mb-3 text-base font-semibold">Constraints</h3><ul class="space-y-2">{props.detail.constraints.map((constraint) => <li class="rounded-md border border-slate-200 p-3 dark:border-slate-800"><p class="font-medium">{constraint.name} <span class="text-xs uppercase text-slate-500">{constraint.kind}</span></p><p class="mt-1 break-all font-mono text-xs text-slate-600 dark:text-slate-300">{constraint.definition}</p></li>)}</ul></section>
          <section><h3 class="mb-3 text-base font-semibold">Foreign keys</h3><ul class="space-y-2">{props.detail.foreign_keys.map((foreign_key) => <li class="rounded-md border border-slate-200 p-3 text-sm dark:border-slate-800"><p class="font-medium">{foreign_key.name}</p><p class="mt-1 font-mono text-xs text-slate-600 dark:text-slate-300">{foreign_key.columns.map((column) => <span>{column} </span>)} → {foreign_key.referenced.schema}.{foreign_key.referenced.name} ({foreign_key.referenced_columns.map((column) => <span>{column} </span>)})</p><p class="mt-1 text-xs text-slate-500">ON UPDATE {foreign_key.on_update} · ON DELETE {foreign_key.on_delete}</p></li>)}</ul></section>
        </div>
      )}
      {props.detail && props.ddl_active && <section><h3 class="mb-3 text-base font-semibold">DDL</h3><textarea aria-label="Read-only table DDL" data-sql-editor data-readonly="true" class="sr-only">{props.detail.ddl}</textarea></section>}
      {props.detail && props.views_active && <section><h3 class="mb-3 text-base font-semibold">Engine views for this relation</h3>{props.no_relation_views ? <p class="text-sm text-slate-500 dark:text-slate-400">No relation-scoped engine views are available.</p> : <ul class="space-y-2">{props.relation_views.map((view) => <li><a class="text-sky-700 hover:underline dark:text-sky-300" href={view.href}>{view.title}</a></li>)}</ul>}</section>}
    </section>
  );
}
