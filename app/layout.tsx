import Notice from "./components/notice.tsx";

export default function RootLayout(props) {
  return (
    <>
      <Head>
        <title>{props.title || "pgpaint"}</title>
        <meta name="csrf-token" content={props.csrf_token || ""} />
      </Head>
      <div class="min-h-screen bg-white text-slate-950 dark:bg-slate-950 dark:text-slate-100">
        <header class="border-b border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-950">
          <nav class="mx-auto flex h-16 max-w-screen-2xl items-center gap-8 px-5" aria-label="Main navigation">
            <a class="text-lg font-bold tracking-tight" href="/">pgpaint</a>
            <a class="text-sm font-medium text-slate-600 hover:text-slate-950 dark:text-slate-300 dark:hover:text-white" href="/">Connections</a>
            <a class="text-sm font-medium text-slate-600 hover:text-slate-950 dark:text-slate-300 dark:hover:text-white" href="/history">History</a>
            <a class="text-sm font-medium text-slate-600 hover:text-slate-950 dark:text-slate-300 dark:hover:text-white" href="/saved">Saved</a>
            {props.auth_enabled && props.user && (
              <div data-auth-status-root class="ml-auto flex items-center gap-4">
                <span class="text-sm text-slate-500 dark:text-slate-400">{props.user}</span>
                {props.passkeys_enabled && <a class="text-sm font-medium text-slate-600 hover:text-slate-950 dark:text-slate-300 dark:hover:text-white" href="/account/passkeys">Passkeys</a>}
                <form method="post" action="/auth/logout" data-auth-logout>
                  <input type="hidden" name="_csrf" value={props.csrf_token} />
                  <button type="submit" class="text-sm font-medium text-slate-600 hover:text-slate-950 dark:text-slate-300 dark:hover:text-white">Log out</button>
                </form>
                <span data-auth-status role="status" aria-live="polite" class="text-xs text-rose-700 dark:text-rose-300"></span>
              </div>
            )}
            {props.auth_enabled && !props.user && <a class="ml-auto text-sm font-medium text-sky-700 hover:text-sky-600 dark:text-sky-300" href="/login">Sign in</a>}
          </nav>
        </header>
        <Notice notice={props.flash} />
        <div>{props.children}</div>
        <script src="/public/assets/app.js"></script>
      </div>
    </>
  );
}
