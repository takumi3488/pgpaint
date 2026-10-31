import CsrfToken from "../../../components/csrf_token.tsx";
import DbError from "../../../components/db_error.tsx";

export default function Explain(props: {
  csrf_token: string;
  profile_id: string;
  sql: string;
  message: string | null;
  error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null;
  confirmation_required: boolean;
  has_result: boolean;
  plan_available: boolean;
  analyzed: boolean;
  raw: string;
  plan: Array<{
    indent: string;
    node_type: string;
    relation: string | null;
    estimated_rows: string;
    actual_rows: string;
    misestimated: boolean;
    cost: string;
    cost_width: string;
    has_cost: boolean;
    actual_time: string;
    time_width: string;
    has_actual_time: boolean;
    loops: string;
    details: Array<{ name: string; value: string }>;
    has_details: boolean;
  }>;
}) {
  return (
    <section class="space-y-4">
      {props.message && <p class="rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-100" role="alert">{props.message}</p>}
      {props.error && <DbError error={props.error} />}
      {props.confirmation_required && (
        <form class="rounded-lg border border-amber-300 bg-amber-50 p-4 text-sm text-amber-950 dark:border-amber-900 dark:bg-amber-950 dark:text-amber-100" hx-post={"/c/" + props.profile_id + "/explain"} hx-target="#query-explain-result" hx-swap="innerHTML">
          <CsrfToken token={props.csrf_token} />
          <input type="hidden" name="sql" value={props.sql} />
          <input type="hidden" name="analyze" value="1" />
          <input type="hidden" name="confirm" value="1" />
          <p>EXPLAIN ANALYZE executes this DML or DDL statement. Confirm only if you intend to run it.</p>
          <button class="mt-3 rounded-md bg-amber-700 px-3 py-2 font-medium text-white hover:bg-amber-800 dark:bg-amber-600 dark:hover:bg-amber-500" type="submit">Confirm and run ANALYZE</button>
        </form>
      )}
      {props.has_result && (
        <div class="space-y-4">
          <p class="text-sm text-slate-600 dark:text-slate-300">{props.analyzed ? "EXPLAIN ANALYZE plan" : "EXPLAIN plan"}</p>
          <div class="space-y-3 explain-tabs">
            <input class="sr-only" type="radio" name="explain-view" id="explain-view-plan" checked />
            <input class="sr-only" type="radio" name="explain-view" id="explain-view-raw" />
            <div class="explain-tablist flex gap-2 border-b border-slate-200 dark:border-slate-800" role="group" aria-label="Explain output">
              <label class="cursor-pointer border-b-2 border-transparent px-3 py-2 text-sm text-slate-500" for="explain-view-plan">Plan tree</label>
              <label class="cursor-pointer border-b-2 border-transparent px-3 py-2 text-sm text-slate-500" for="explain-view-raw">Raw</label>
            </div>
            <div class="explain-panels">
              <div class="plan-panel space-y-3">
              {props.plan_available ? props.plan.map((node) => (
                <article class="rounded-lg border border-slate-200 bg-white p-3 dark:border-slate-800 dark:bg-slate-950">
                  <div class="flex min-w-0 items-start gap-2">
                    <span class="shrink-0 whitespace-pre font-mono text-xs text-slate-400" aria-hidden="true">{node.indent}</span>
                    <div class="min-w-0 flex-1">
                      <h3 class="font-mono text-sm font-semibold text-slate-900 dark:text-slate-100">{node.node_type}{node.relation && <span class="ml-2 font-normal text-slate-600 dark:text-slate-400">· {node.relation}</span>}</h3>
                      <p class={node.misestimated ? "mt-1 text-sm font-medium text-amber-700 dark:text-amber-300" : "mt-1 text-sm text-slate-600 dark:text-slate-300"}>
                        Estimated rows: {node.estimated_rows} <span aria-hidden="true">·</span> Actual rows: {node.actual_rows} <span aria-hidden="true">·</span> Loops: {node.loops}
                      </p>
                      <div class="mt-3 grid gap-3 sm:grid-cols-2">
                        <div>
                          <div class="flex justify-between text-xs text-slate-500 dark:text-slate-400"><span>Cost (relative to root)</span><span>{node.cost}</span></div>
                          <div class="mt-1 h-1.5 overflow-hidden rounded bg-slate-100 dark:bg-slate-800" role="img" aria-label={"Cost " + node.cost + " relative to root"}>
                            {node.has_cost && <div class="h-full rounded bg-sky-500" style={node.cost_width}></div>}
                          </div>
                        </div>
                        <div>
                          <div class="flex justify-between text-xs text-slate-500 dark:text-slate-400"><span>Actual time (relative to root)</span><span>{node.actual_time} ms</span></div>
                          <div class="mt-1 h-1.5 overflow-hidden rounded bg-slate-100 dark:bg-slate-800" role="img" aria-label={"Actual time " + node.actual_time + " milliseconds relative to root"}>
                            {node.has_actual_time && <div class="h-full rounded bg-emerald-500" style={node.time_width}></div>}
                          </div>
                        </div>
                      </div>
                      {node.has_details && <details class="mt-3 text-sm">
                        <summary class="cursor-pointer text-sky-700 hover:underline dark:text-sky-300">Node details</summary>
                        <dl class="mt-2 space-y-1 rounded bg-slate-50 p-3 dark:bg-slate-900">
                          {node.details.map((detail) => <div class="grid gap-1 sm:grid-cols-[minmax(8rem,auto)_1fr]"><dt class="font-medium text-slate-600 dark:text-slate-300">{detail.name}</dt><dd class="break-all font-mono text-xs text-slate-700 dark:text-slate-200">{detail.value}</dd></div>)}
                        </dl>
                      </details>}
                    </div>
                  </div>
                </article>
              )) : <p class="rounded-lg bg-slate-50 p-4 text-sm text-slate-600 dark:bg-slate-900 dark:text-slate-300">The server output could not be normalized into a plan tree. See Raw for the original output.</p>}
            </div>
            <div class="raw-panel" role="tabpanel">
              <pre class="max-h-[60vh] overflow-auto rounded-lg bg-slate-950 p-4 text-xs text-slate-100">{props.raw}</pre>
            </div>
            </div>
            <style>{"#explain-view-plan:checked ~ .explain-panels .plan-panel, #explain-view-raw:checked ~ .explain-panels .raw-panel { display: block; } .explain-panels .plan-panel, .explain-panels .raw-panel { display: none; } #explain-view-plan:checked ~ .explain-tablist label[for='explain-view-plan'], #explain-view-raw:checked ~ .explain-tablist label[for='explain-view-raw'] { border-color: #0284c7; color: #0369a1; } @media (prefers-color-scheme: dark) { #explain-view-plan:checked ~ .explain-tablist label[for='explain-view-plan'], #explain-view-raw:checked ~ .explain-tablist label[for='explain-view-raw'] { color: #7dd3fc; } }"}</style>
          </div>
        </div>
      )}
    </section>
  );
}
