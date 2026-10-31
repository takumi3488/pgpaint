export default function Notice(props: { notice: { kind: string; message: string } | null }) {
  return <div class="empty:hidden mx-auto mt-4 max-w-screen-2xl rounded-lg border border-sky-300 bg-sky-50 px-4 py-3 text-sm text-sky-950 dark:border-sky-900 dark:bg-sky-950 dark:text-sky-100" role="status">{props.notice && props.notice.message}</div>;
}
