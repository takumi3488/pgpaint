export default function ErDiagramPage(props) {
  return (
    <section data-er-diagram data-graph-url={props.graph_url} data-table-url-prefix={props.table_url_prefix} data-schema={props.schema} class="space-y-4" aria-labelledby="er-title">
      <div class="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h2 id="er-title" class="text-xl font-semibold">ER diagram</h2>
          <p class="mt-1 text-sm text-slate-500 dark:text-slate-400">Tables and relationships in <span class="font-medium text-slate-700 dark:text-slate-200">{props.schema}</span>.</p>
        </div>
        <label class="flex items-center gap-2 text-sm font-medium text-slate-700 dark:text-slate-300" for="er-schema-select">
          Schema
          <select id="er-schema-select" data-er-schema-select class="rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950">
            {props.schemas.map((schema) => (
              <option value={schema.href} selected={schema.selected}>{schema.name}</option>
            ))}
          </select>
        </label>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <label class="sr-only" for="er-search">Filter and highlight tables</label>
        <input id="er-search" data-er-search type="search" placeholder="Find table…" autocomplete="off" class="w-48 rounded-md border border-slate-300 bg-white px-3 py-2 text-sm dark:border-slate-700 dark:bg-slate-950" />
        <button type="button" data-er-fit class="rounded-md border border-slate-300 px-3 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900">Fit</button>
        <button type="button" data-er-export-svg class="rounded-md border border-slate-300 px-3 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900">Export SVG</button>
        <button type="button" data-er-export-png class="rounded-md border border-slate-300 px-3 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900">Export PNG</button>
      </div>
      <p data-er-status role="status" aria-live="polite" class="text-sm text-slate-500 dark:text-slate-400">Loading diagram…</p>
      <div class="flex items-center gap-4 text-xs text-slate-600 dark:text-slate-300" aria-label="Diagram legend">
        <span class="flex items-center gap-1"><span class="font-semibold">PK</span> Primary key</span>
        <span class="flex items-center gap-1"><span class="font-semibold">FK</span> Foreign key</span>
        <span class="flex items-center gap-2"><svg aria-hidden="true" width="28" height="8" viewBox="0 0 28 8"><path d="M0 4H28" stroke="currentColor" stroke-dasharray="4 3" /></svg> Inferred relationship</span>
      </div>
      <div data-er-canvas role="region" aria-label="Interactive entity relationship diagram" tabindex="0" class="h-[65vh] min-h-[28rem] overflow-hidden rounded-lg border border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-950">
        <p class="p-6 text-sm text-slate-500 dark:text-slate-400">The diagram will appear here when JavaScript is enabled.</p>
      </div>
    </section>
  );
}
