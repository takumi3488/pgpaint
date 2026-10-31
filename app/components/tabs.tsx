export default function Tabs(props: { tabs: Array<{ label: string; href: string; active: boolean }> }) {
  return (
    <nav class="flex min-w-0 gap-1 overflow-x-auto border-b border-slate-200 px-4 dark:border-slate-800" aria-label="Workspace tabs">
      {props.tabs.map((tab) => (
        <a href={tab.href} class={tab.active ? "whitespace-nowrap border-b-2 border-sky-600 px-3 py-3 text-sm font-medium text-sky-700 dark:text-sky-300" : "whitespace-nowrap border-b-2 border-transparent px-3 py-3 text-sm text-slate-600 hover:text-slate-950 dark:text-slate-400 dark:hover:text-white"}>
          {tab.label}
        </a>
      ))}
    </nav>
  );
}
