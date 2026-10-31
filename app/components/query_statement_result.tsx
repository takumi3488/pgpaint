import DataGrid from "./data_grid.tsx";

type QueryStatement = {
  label: string;
  summary: string;
  truncated: boolean;
  has_columns: boolean;
  columns: Array<{ label: string; type_name: string }>;
  rows: Array<{ cells: Array<{ value: string; is_null: boolean }> }>;
  page: number;
  total_pages: number;
  previous_href: string | null;
  next_href: string | null;
  csv_href: string | null;
  json_href: string | null;
};

export default function QueryStatementResult(props: { statement: QueryStatement }) {
  return (
    <section class="space-y-3" aria-label={props.statement.label}>
      <div class="flex flex-wrap items-center justify-between gap-3">
        <p class="text-sm text-slate-600 dark:text-slate-300">{props.statement.summary}</p>
        <div class="flex items-center gap-3 text-sm">
          {props.statement.csv_href && <a class="font-medium text-sky-700 hover:underline dark:text-sky-300" href={props.statement.csv_href}>Export CSV</a>}
          {props.statement.json_href && <a class="font-medium text-sky-700 hover:underline dark:text-sky-300" href={props.statement.json_href}>Export JSON</a>}
        </div>
      </div>
      {props.statement.truncated && <p class="rounded-md bg-amber-100 px-3 py-2 text-sm text-amber-950 dark:bg-amber-950 dark:text-amber-100">Results truncated at the in-memory row or size limit.</p>}
      {props.statement.has_columns && <DataGrid columns={props.statement.columns} rows={props.statement.rows} hide_actions />}
      <nav class="flex items-center justify-between text-sm" aria-label="Result pages">
        <div>{props.statement.previous_href && <a class="rounded-md border border-slate-300 px-3 py-2 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800" href={props.statement.previous_href} hx-get={props.statement.previous_href} hx-params="none" hx-target="#query-run-result" hx-swap="outerHTML">Previous</a>}</div>
        <span class="text-slate-600 dark:text-slate-400">Page {props.statement.page} of {props.statement.total_pages}</span>
        <div>{props.statement.next_href && <a class="rounded-md border border-slate-300 px-3 py-2 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800" href={props.statement.next_href} hx-get={props.statement.next_href} hx-params="none" hx-target="#query-run-result" hx-swap="outerHTML">Next</a>}</div>
      </nav>
    </section>
  );
}
