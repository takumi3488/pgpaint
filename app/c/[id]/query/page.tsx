import CsrfToken from "../../../components/csrf_token.tsx";
import LintIssues from "../../../components/lint_issues.tsx";
import QueryRunResult from "../../../components/query_run_result.tsx";

export default function QueryPage(props) {
  return (
    <main class="space-y-6">
      <section class="rounded-xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <div class="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 class="text-2xl font-semibold">SQL editor</h2>
            <p class="mt-1 text-sm text-slate-600 dark:text-slate-400">Run SQL against {props.profile_name}. Use Ctrl/Cmd+Enter to run.</p>
          </div>
          {props.read_only && <p class="rounded-md bg-amber-100 px-3 py-1.5 text-sm font-medium text-amber-900 dark:bg-amber-950 dark:text-amber-100">Read-only profile</p>}
        </div>

        <form id="query-form" method="post" action={props.query_url} class="space-y-4">
          <CsrfToken token={props.csrf_token} />
          <label class="block">
            <span class="sr-only">SQL query</span>
            <textarea name="sql" data-sql-editor data-completion-url={props.completion_url} aria-label="SQL query" hidden>{props.sql}</textarea>
          </label>
          <div class="flex flex-wrap items-center gap-2">
            <button class="rounded-md bg-sky-700 px-4 py-2 text-sm font-semibold text-white hover:bg-sky-600" type="button" hx-post={props.query_url} hx-include="closest form" hx-target="#query-run-result" hx-select="#query-run-result" hx-swap="outerHTML">Run</button>
            <button class="rounded-md border border-slate-300 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800" type="button" hx-post={props.explain_url} hx-include="closest form" hx-target="#query-explain-result" hx-swap="innerHTML">Explain</button>
            {props.can_explain_analyze && <button class="rounded-md border border-amber-300 px-4 py-2 text-sm font-semibold text-amber-900 hover:bg-amber-50 dark:border-amber-800 dark:text-amber-100 dark:hover:bg-amber-950" type="button" name="analyze" value="1" hx-post={props.explain_url} hx-include="closest form" hx-target="#query-explain-result" hx-swap="innerHTML">Explain Analyze</button>}
            <details class="relative">
              <summary class="cursor-pointer list-none rounded-md border border-slate-300 px-4 py-2 text-sm font-semibold text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800">Save query</summary>
              <div class="absolute right-0 z-10 mt-2 w-72 space-y-3 rounded-lg border border-slate-200 bg-white p-4 shadow-lg dark:border-slate-700 dark:bg-slate-900">
                <label class="block space-y-1 text-sm font-medium">
                  <span>Query name</span>
                  <input name="name" maxlength="120" autocomplete="off" class="w-full rounded-md border border-slate-300 bg-white px-3 py-2 text-slate-950 dark:border-slate-700 dark:bg-slate-950 dark:text-white" />
                </label>
                <button class="w-full rounded-md bg-slate-800 px-3 py-2 text-sm font-semibold text-white hover:bg-slate-700 dark:bg-slate-100 dark:text-slate-900" type="submit" name="action" value="save" formaction={props.query_url}>Save query</button>
              </div>
            </details>
          </div>

      <section aria-label="Query results" aria-live="polite">
        {props.run && <QueryRunResult run={props.run} />}
        {!props.run && props.has_lint_issues && (
          <div id="query-run-result" class="space-y-4 rounded-xl border border-amber-300 bg-amber-50 p-5 dark:border-amber-900 dark:bg-amber-950">
            <h3 class="font-semibold text-amber-950 dark:text-amber-100">Review these lint issues</h3>
            <LintIssues issues={props.lint_issues} />
            {props.force_available && <button class="rounded-md bg-amber-800 px-4 py-2 text-sm font-semibold text-white hover:bg-amber-700 dark:bg-amber-200 dark:text-amber-950" type="button" name="force" value="true" hx-post={props.query_url} hx-include="closest form" hx-target="#query-run-result" hx-select="#query-run-result" hx-swap="outerHTML">Run anyway</button>}
            {!props.force_available && props.read_only && <p class="text-sm font-medium text-amber-950 dark:text-amber-100">This profile is read-only. Read-only violations cannot be overridden.</p>}
          </div>
        )}
        {!props.run && !props.has_lint_issues && <div id="query-run-result" class="rounded-xl border border-dashed border-slate-300 p-5 text-sm text-slate-500 dark:border-slate-700 dark:text-slate-400">Results will appear here.</div>}
      </section>
        </form>
      </section>

      <section id="query-explain-result" class="space-y-4" aria-label="Explain plan" aria-live="polite"></section>
    </main>
  );
}
