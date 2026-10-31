export default function Pager(props: { page: number; total_pages: number; previous_href?: string | null; next_href?: string | null }) {
  return (
    <nav class="flex items-center justify-between text-sm" aria-label="Pagination">
      <div>{props.previous_href && <a class="rounded-md border border-slate-300 px-3 py-2 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900" href={props.previous_href}>Previous</a>}</div>
      <span class="text-slate-600 dark:text-slate-400">Page {props.page} of {props.total_pages}</span>
      <div>{props.next_href && <a class="rounded-md border border-slate-300 px-3 py-2 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-900" href={props.next_href}>Next</a>}</div>
    </nav>
  );
}
