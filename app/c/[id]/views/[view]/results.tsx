import DataGrid from "../../../../components/data_grid.tsx";
import DbError from "../../../../components/db_error.tsx";

export default function EngineViewResults(props: {
  result_loaded: boolean;
  error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null;
  columns: Array<{ label: string; type_name: string }>;
  rows: Array<{ cells: Array<{ value: string; is_null: boolean }> }>;
  truncated: boolean;
  is_activity: boolean;
  refresh_url: string;
}) {
  return (
    <div id="view-results" class="space-y-3" hx-get={props.refresh_url} hx-trigger={props.is_activity ? "every 5s [document.getElementById('view-auto-refresh').checked]" : ""} hx-target="#view-page" hx-swap="outerHTML">
      {props.error && <DbError error={props.error} />}
      {props.result_loaded && <DataGrid columns={props.columns} rows={props.rows} hide_actions={true} />}
      {props.truncated && <p class="text-sm text-amber-700 dark:text-amber-300">The result was truncated at the configured row limit.</p>}
    </div>
  );
}
