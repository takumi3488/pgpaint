export default function EngineBadge(props: { engine: string }) {
  return (
    <span class="inline-flex items-center rounded-full border border-slate-300 bg-slate-100 px-2.5 py-1 text-xs font-medium text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200">
      {props.engine}
    </span>
  );
}
