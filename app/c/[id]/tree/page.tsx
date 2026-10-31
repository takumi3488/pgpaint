import DbError from "../../../components/db_error.tsx";

export default function SchemaTreePage(props: { url: string; filter: string; show_empty: boolean; schemas: Array<{ name: string; groups: Array<{ label: string; relations: Array<{ name: string; href: string }> }>; sequences: string[]; sequences_visible: boolean; routines: string[]; routines_visible: boolean; routines_empty: boolean }>; error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null }) {
  return (
    <div id="schema-tree" hx-get={props.url} hx-trigger="pgpaint:schema-changed from:body" hx-include="#schema-filter" hx-swap="outerHTML" class="space-y-3 text-sm">
      <div class="flex items-center justify-between gap-2">
        <label class="sr-only" for="schema-filter">Filter schema objects</label>
        <input id="schema-filter" name="filter" type="search" value={props.filter} aria-label="Filter schemas and objects" placeholder="Filter schemas and objects" hx-get={props.url} hx-trigger="input changed delay:250ms, search" hx-target="#schema-tree" hx-swap="outerHTML" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900 placeholder:text-slate-400 dark:border-slate-700 dark:bg-slate-950 dark:text-slate-100" />
      </div>
      <DbError error={props.error} />
      {props.show_empty && !props.error && <p class="py-2 text-slate-500 dark:text-slate-400">No matching schema objects.</p>}
      {props.schemas.map((schema) => (
        <details class="rounded-md border border-slate-200 dark:border-slate-800">
          <summary class="cursor-pointer px-3 py-2 font-medium text-slate-800 dark:text-slate-100">{schema.name}</summary>
          <div class="space-y-2 border-t border-slate-200 p-2 dark:border-slate-800">
            {schema.groups.map((group) => (
              <details class="pl-2">
                <summary class="cursor-pointer py-1 text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">{group.label}</summary>
                <ul class="space-y-1 pl-3">
                  {group.relations.map((relation) => <li><a class="break-all text-sky-700 hover:underline dark:text-sky-300" href={relation.href}>{relation.name}</a></li>)}
                </ul>
              </details>
            ))}
            {schema.sequences_visible && <details class="pl-2"><summary class="cursor-pointer py-1 text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">Sequences</summary><ul class="space-y-1 pl-3">{schema.sequences.map((sequence) => <li class="break-all text-slate-700 dark:text-slate-300">{sequence}</li>)}</ul></details>}
            {schema.routines_visible && <details class="pl-2"><summary class="cursor-pointer py-1 text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">Routines</summary><ul class="space-y-1 pl-3">{schema.routines.map((routine) => <li class="break-all text-slate-700 dark:text-slate-300">{routine}</li>)}{schema.routines_empty && <li class="text-slate-400">No routines.</li>}</ul></details>}
          </div>
        </details>
      ))}
    </div>
  );
}
