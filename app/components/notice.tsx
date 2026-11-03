export default function Notice(props: { notice: { kind: string; message: string } | null }) {
  return <div class="empty:hidden mx-auto mt-4 max-w-screen-2xl rounded-lg border border-orange-300 bg-orange-50 px-4 py-3 text-sm text-orange-950 dark:border-orange-900 dark:bg-orange-950 dark:text-orange-100" role="status">{props.notice && props.notice.message}</div>;
}
