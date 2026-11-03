export default function EngineBadge(props: { engine: string }) {
  return (
    <span class="inline-flex items-center rounded-full border border-stone-300 bg-stone-100 px-2.5 py-1 text-xs font-medium text-stone-700 dark:border-stone-700 dark:bg-stone-800 dark:text-stone-200">
      {props.engine}
    </span>
  );
}
