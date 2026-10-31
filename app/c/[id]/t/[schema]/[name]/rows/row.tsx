export default function SchemaRow(props: { url: string; out_of_band: boolean; row: { id: string; cells: Array<{ value: string; is_null: boolean }>; key_fields: Array<{ value_name: string; value: string }>; edit_fields: Array<{ name: string; value_name: string; null_name: string; value: string; is_null: boolean; nullable: boolean }>; editable: boolean; can_update: boolean; editing: boolean; edit_values: string; cancel_values: string } }) {
  return (
    <tr id={props.row.id} hx-swap-oob={props.out_of_band ? "outerHTML" : null} class="bg-white dark:bg-slate-950">
      {props.row.editing && (
        <td colspan={props.row.cells.length + 1} class="p-3">
          <form hx-put={props.url} hx-target="closest tr" hx-swap="none" hx-include="#row-filter-form" class="space-y-3">
            {props.row.key_fields.map((key) => <input type="hidden" name={key.value_name} value={key.value} />)}
            <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {props.row.edit_fields.map((field) => (
                <label class="block text-xs font-medium text-slate-600 dark:text-slate-300">
                  <span class="mb-1 block">{field.name}</span>
                  <input aria-label={field.name} name={field.value_name} value={field.value} class="w-full rounded-md border border-slate-300 bg-white px-2 py-1.5 font-mono text-sm text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100" />
                  {field.nullable && <span class="mt-1 flex items-center gap-1"><input type="checkbox" name={field.null_name} value="true" checked={field.is_null} /><span>NULL</span></span>}
                </label>
              ))}
            </div>
            <div class="flex gap-2">
              <button type="submit" class="rounded-md bg-sky-700 px-3 py-1.5 text-xs font-semibold text-white hover:bg-sky-600">Save</button>
              <button type="button" hx-get={props.url} hx-vals={props.row.cancel_values} hx-target="closest tr" hx-swap="none" hx-include="#row-filter-form" class="rounded-md border border-slate-300 px-3 py-1.5 text-xs font-medium dark:border-slate-700">Cancel</button>
            </div>
          </form>
        </td>
      )}
      {!props.row.editing && props.row.cells.map((cell) => <td class="whitespace-nowrap px-3 py-2 text-sm text-slate-700 dark:text-slate-200">{cell.is_null ? <span class="italic text-slate-400">NULL</span> : cell.value}</td>)}
      {!props.row.editing && (
        <td class="whitespace-nowrap px-3 py-2">
          {props.row.can_update && <button type="button" hx-get={props.url} hx-vals={props.row.edit_values} hx-target="closest tr" hx-swap="none" hx-include="#row-filter-form" class="mr-2 text-xs font-medium text-sky-700 hover:underline dark:text-sky-300">Edit</button>}
          {props.row.editable && (
            <form class="inline" hx-delete={props.url} hx-target="#rows-fragment" hx-swap="outerHTML" hx-include="#row-filter-form" hx-confirm="Delete this row?">
              {props.row.key_fields.map((key) => <input type="hidden" name={key.value_name} value={key.value} />)}
              <button type="submit" class="text-xs font-medium text-red-700 hover:underline dark:text-red-300">Delete</button>
            </form>
          )}
        </td>
      )}
    </tr>
  );
}
