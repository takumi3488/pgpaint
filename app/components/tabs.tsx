export default function Tabs(props: { tabs: Array<{ label: string; href: string; active: boolean }> }) {
  return (
    <nav class="flex min-w-0 gap-1 overflow-x-auto border-b border-stone-200 px-4 dark:border-stone-800" aria-label="Workspace tabs">
      {props.tabs.map((tab) => (
        <a href={tab.href} class={tab.active ? "whitespace-nowrap border-b-2 border-orange-800 dark:border-orange-300 px-3 py-3 text-sm font-medium text-orange-700 dark:text-orange-300" : "whitespace-nowrap border-b-2 border-transparent px-3 py-3 text-sm text-stone-600 hover:text-stone-950 dark:text-stone-400 dark:hover:text-white"}>
          {tab.label}
        </a>
      ))}
    </nav>
  );
}
