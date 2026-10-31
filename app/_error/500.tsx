export default function InternalError(props: { status: number; message: string }) {
  return <main class="mx-auto max-w-screen-2xl px-5 py-16"><h1 class="text-3xl font-semibold">Something went wrong</h1><p class="mt-3 text-slate-600 dark:text-slate-400">{props.message}</p></main>;
}
