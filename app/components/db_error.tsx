export default function DbError(props: { error: { kind: string; message: string; code?: string | null; detail?: string | null; hint?: string | null } | null }) {
  return (
    <section class="empty:hidden rounded-lg border border-red-300 bg-red-50 p-4 text-sm text-red-900 dark:border-red-900 dark:bg-red-950 dark:text-red-100" role="alert">
      {props.error && <p class="font-semibold">{props.error.message}</p>}
      {props.error && props.error.code && <p class="mt-1 font-mono text-xs">SQLSTATE {props.error.code}</p>}
      {props.error && props.error.detail && <p class="mt-2">{props.error.detail}</p>}
      {props.error && props.error.hint && <p class="mt-2">Hint: {props.error.hint}</p>}
    </section>
  );
}
