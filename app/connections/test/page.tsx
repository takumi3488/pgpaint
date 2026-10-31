import DbError from "../../components/db_error.tsx";
import EngineBadge from "../../components/engine_badge.tsx";

export default function TestConnectionPage(props) {
  return (
    <section class="mt-5" aria-live="polite">
      {props.server_info && (
        <div class="rounded-lg border border-emerald-300 bg-emerald-50 p-4 text-sm text-emerald-950 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-100" role="status">
          <div class="flex flex-wrap items-center gap-2">
            <h2 class="font-semibold">Connection successful</h2>
            <EngineBadge engine={props.server_info.engine_label} />
          </div>
          <p class="mt-2">{props.server_info.version}</p>
          <p class="mt-1">Database: {props.server_info.database} · User: {props.server_info.user}</p>
          {props.server_info.mismatch_warning && <p class="mt-3 rounded-md border border-amber-400 bg-amber-100 p-3 font-medium text-amber-950 dark:border-amber-800 dark:bg-amber-900 dark:text-amber-100" role="alert">{props.server_info.mismatch_warning}</p>}
        </div>
      )}
      {props.error && <DbError error={props.error} />}
    </section>
  );
}
