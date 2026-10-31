export default function DataGrid(props: { columns: Array<{ label: string; type_name?: string }>; rows: Array<{ cells: Array<{ value: string; is_null: boolean }>; actions?: Array<{ label: string; href: string }> }>; hide_actions?: boolean }) {
  return (
    <div class="min-w-0 max-w-full overflow-x-auto rounded-lg border border-slate-200 dark:border-slate-800" style="contain: paint">
      <table class="min-w-full divide-y divide-slate-200 text-sm dark:divide-slate-800">
        <thead class="bg-slate-50 text-left text-xs uppercase tracking-wide text-slate-500 dark:bg-slate-900 dark:text-slate-400">
          <tr>
            {props.columns.map((column) => <th class="px-4 py-3 font-medium" title={column.type_name}>{column.label}{column.type_name && <span class="sr-only">, type {column.type_name}</span>}</th>)}
            {!props.hide_actions && <th class="px-4 py-3 font-medium">Actions</th>}
          </tr>
        </thead>
        <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
          {props.rows.map((row) => (
            <tr class="bg-white dark:bg-slate-950">
              {row.cells.map((cell) => <td class="whitespace-nowrap px-4 py-3 text-slate-700 dark:text-slate-200">{cell.is_null ? <span class="italic text-slate-400">NULL</span> : cell.value}</td>)}
              {!props.hide_actions && <td class="whitespace-nowrap px-4 py-3">{row.actions && row.actions.map((action) => <a class="mr-3 text-sky-700 hover:underline dark:text-sky-300" href={action.href}>{action.label}</a>)}</td>}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
