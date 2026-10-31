export default function NotFound(props: { status: number; message: string }) {
  return <main class="mx-auto max-w-screen-2xl px-5 py-16"><h1 class="text-3xl font-semibold">Page not found</h1><p class="mt-3 text-slate-600 dark:text-slate-400">{props.message}</p><a class="mt-6 inline-block text-sky-700 hover:underline dark:text-sky-300" href="/">Return to connections</a></main>;
}
