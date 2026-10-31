import DbError from "../../../../../../components/db_error.tsx";
import SchemaRow from "./row.tsx";

export default function RowsPage(props: { url: string; row_only: boolean; row_id: string; row: null | { id: string; cells: Array<{ value: string; is_null: boolean }>; key_fields: Array<{ value_name: string; value: string }>; edit_fields: Array<{ name: string; value_name: string; null_name: string; value: string; is_null: boolean; nullable: boolean }>; editable: boolean; can_update: boolean; editing: boolean; edit_values: string; cancel_values: string }; rows: Array<{ id: string; cells: Array<{ value: string; is_null: boolean }>; key_fields: Array<{ value_name: string; value: string }>; edit_fields: Array<{ name: string; value_name: string; null_name: string; value: string; is_null: boolean; nullable: boolean }>; editable: boolean; can_update: boolean; editing: boolean; edit_values: string; cancel_values: string }>; columns: Array<{ name: string; data_type: string; index: number; filter_name: string; value_name: string; filter_value: string; filter_options: Array<{ value: string; label: string; selected: boolean }>; sort_values: string; sorted: boolean; descending: boolean }>; insert_fields: Array<{ name: string; value_name: string; null_name: string; default_name: string; value: string; nullable: boolean; has_default: boolean; use_default: boolean }>; editable: boolean; page: number; offset: number; row_count: number; sort_column_value: string; sort_descending_value: string; show_previous: boolean; previous_values: string; next_values: string; has_more: boolean; no_rows: boolean; estimate_label: string; error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null }) {
  return (
    <div id="rows-fragment" class="space-y-4">
      {props.row_only && props.row && !props.error && <table><tbody><SchemaRow url={props.url} row={props.row} out_of_band={true} /></tbody></table>}
      {props.row_only && props.error && <table><tbody><tr id={props.row_id} hx-swap-oob="outerHTML"><td colspan={100} class="p-3"><DbError error={props.error} /></td></tr></tbody></table>}
      {!props.row_only && (
        <section class="space-y-4">
          <DbError error={props.error} />
          <form id="row-filter-form" hx-get={props.url} hx-trigger="input changed delay:300ms" hx-target="#rows-fragment" hx-swap="outerHTML" hx-vals='{"page":0}' class="grid gap-3 rounded-lg border border-slate-200 p-3 dark:border-slate-800 sm:grid-cols-2 xl:grid-cols-3">
            <input type="hidden" name="page" value={props.page} />
            <input type="hidden" name="sort_column" value={props.sort_column_value} />
            <input type="hidden" name="sort_descending" value={props.sort_descending_value} />
            {props.columns.map((column) => (
              <fieldset class="min-w-0">
                <legend class="mb-1 text-xs font-semibold text-slate-600 dark:text-slate-300">Filter {column.name}</legend>
                <div class="flex gap-2">
                  <select name={column.filter_name} aria-label={"Filter operator for " + column.name} class="min-w-0 rounded-md border border-slate-300 bg-white px-2 py-1.5 text-xs dark:border-slate-700 dark:bg-slate-900">
                    {column.filter_options.map((option) => <option value={option.value} selected={option.selected}>{option.label}</option>)}
                  </select>
                  <input name={column.value_name} value={column.filter_value} aria-label={"Filter value for " + column.name} class="min-w-0 flex-1 rounded-md border border-slate-300 bg-white px-2 py-1.5 text-xs dark:border-slate-700 dark:bg-slate-900" />
                </div>
              </fieldset>
            ))}
          </form>
          <div class="overflow-x-auto rounded-lg border border-slate-200 dark:border-slate-800">
            <table class="min-w-full divide-y divide-slate-200 text-sm dark:divide-slate-800">
              <thead class="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500 dark:bg-slate-900 dark:text-slate-400">
                <tr>
                  {props.columns.map((column) => <th class="whitespace-nowrap px-3 py-2"><button type="button" aria-label={"Sort by " + column.name} hx-get={props.url} hx-include="#row-filter-form" hx-vals={column.sort_values} hx-target="#rows-fragment" hx-swap="outerHTML" class="font-semibold hover:text-sky-700 dark:hover:text-sky-300">{column.name}{column.sorted && <span> {column.descending ? "↓" : "↑"}</span>}</button><span class="ml-1 normal-case font-normal text-slate-400">{column.data_type}</span></th>)}
                  <th class="px-3 py-2">Actions</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
                {props.rows.map((row) => <SchemaRow url={props.url} row={row} out_of_band={false} />)}
                {props.no_rows && <tr><td colspan={100} class="px-3 py-8 text-center text-sm text-slate-500 dark:text-slate-400">No rows match this query.</td></tr>}
              </tbody>
            </table>
          </div>
          <div class="flex flex-wrap items-center justify-between gap-3 text-sm">
            <p class="text-slate-500 dark:text-slate-400">Showing {props.row_count} rows from offset {props.offset}{props.estimate_label && " · " + props.estimate_label}</p>
            <div class="flex gap-2">
              {props.show_previous && <button type="button" hx-get={props.url} hx-include="#row-filter-form" hx-vals={props.previous_values} hx-target="#rows-fragment" hx-swap="outerHTML" class="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900">Previous</button>}
              {props.has_more && <button type="button" hx-get={props.url} hx-include="#row-filter-form" hx-vals={props.next_values} hx-target="#rows-fragment" hx-swap="outerHTML" class="rounded-md border border-slate-300 px-3 py-1.5 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900">Next</button>}
            </div>
          </div>
          {props.editable && (
            <form hx-post={props.url} hx-target="#rows-fragment" hx-swap="outerHTML" hx-include="#row-filter-form" class="space-y-3 rounded-lg border border-slate-200 p-4 dark:border-slate-800">
              <h3 class="font-semibold">Insert row</h3>
              <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {props.insert_fields.map((field) => (
                  <label class="block text-xs font-medium text-slate-600 dark:text-slate-300">
                    <span class="mb-1 block">{field.name}</span>
                    <input aria-label={field.name} name={field.value_name} value={field.value} class="w-full rounded-md border border-slate-300 bg-white px-2 py-1.5 font-mono text-sm text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100" />
                    {field.nullable && <span class="mt-1 flex items-center gap-1"><input type="checkbox" name={field.null_name} value="true" /><span>NULL</span></span>}
                    {field.has_default && <span class="mt-1 flex items-center gap-1"><input type="checkbox" name={field.default_name} value="true" checked={field.use_default} /><span>Use default</span></span>}
                  </label>
                ))}
              </div>
              <button type="submit" class="rounded-md bg-sky-700 px-3 py-2 text-sm font-semibold text-white hover:bg-sky-600">Insert</button>
            </form>
          )}
        </section>
      )}
    </div>
  );
}
