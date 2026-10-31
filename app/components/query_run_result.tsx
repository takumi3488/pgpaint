import QueryStatementResult from "./query_statement_result.tsx";
import LintIssues from "./lint_issues.tsx";

type QueryStatement = {
  index: number;
  label: string;
  href: string;
  selected: boolean;
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

type QueryRunResultProps = {
  run: {
    id: string;
    status: string;
    status_label: string;
    is_running: boolean;
    events_url: string;
    cancel_url: string;
    result_url: string;
    schema_event_url: string | null;
    elapsed_ms: number;
    warnings: Array<{ severity: string; message: string }>;
    error: string | null;
    selected_result: QueryStatement | null;
    statements: Array<{ index: number; label: string; href: string; selected: boolean }>;
  };
};


export default function QueryRunResult(props: QueryRunResultProps) {
  return (
    <section id="query-run-result" class="space-y-4 rounded-xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900" aria-live="polite" hx-ext={props.run.is_running ? "sse" : null} sse-connect={props.run.is_running ? props.run.events_url : null} sse-swap={props.run.is_running ? "done,failed,canceled" : null} hx-swap="outerHTML">
      {props.run.is_running && (
        <div class="flex flex-wrap items-center justify-between gap-3">
          <p class="flex items-center gap-2 text-sm font-medium text-sky-800 dark:text-sky-200"><span class="h-2 w-2 animate-pulse rounded-full bg-sky-600" aria-hidden="true"></span>Query is running…</p>
          <button class="rounded-md border border-red-300 px-3 py-1.5 text-sm font-semibold text-red-800 hover:bg-red-50 dark:border-red-900 dark:text-red-200 dark:hover:bg-red-950" type="button" hx-post={props.run.cancel_url} hx-params="none" hx-target="#query-run-result" hx-swap="none">Cancel query</button>
        </div>
      )}
      {!props.run.is_running && <div class="flex flex-wrap items-center justify-between gap-3"><h3 class="font-semibold">{props.run.status_label}</h3><p class="text-sm text-slate-500 dark:text-slate-400">Elapsed: {props.run.elapsed_ms} ms</p></div>}
      {props.run.schema_event_url && <div class="hidden" hx-get={props.run.schema_event_url} hx-params="none" hx-trigger="load" hx-swap="none" aria-hidden="true"></div>}
      <LintIssues issues={props.run.warnings} />
      {props.run.error && <p class="rounded-md border border-red-300 bg-red-50 p-3 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100" role="alert">{props.run.error}</p>}
      <nav class="flex flex-wrap gap-2" aria-label="Query statements">
        {props.run.statements.map((statement) => (
          <a class={statement.selected ? "rounded-md bg-sky-700 px-3 py-1.5 text-sm font-semibold text-white" : "rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"} href={statement.href} hx-get={statement.href} hx-params="none" hx-target="#query-run-result" hx-swap="outerHTML" aria-current={statement.selected ? "page" : "false"}>{statement.label}</a>
        ))}
      </nav>
      {props.run.selected_result && <QueryStatementResult statement={props.run.selected_result} />}
    </section>
  );
}
