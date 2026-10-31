export default function LoginPage(props: {
  auth_enabled: boolean;
  email_otp: boolean;
  passkeys: boolean;
  oauth_providers: string[];
}) {
  return (
    <main class="mx-auto max-w-lg px-5 py-16">
      <section class="rounded-2xl border border-slate-200 bg-white p-8 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <h1 class="text-2xl font-semibold tracking-tight">Sign in to pgpaint</h1>
        {props.auth_enabled ? (
          <div class="mt-6 space-y-6" data-auth-status-root>
            {props.oauth_providers.length > 0 && (
              <div class="space-y-3">
                {props.oauth_providers.map((provider) => (
                  <button type="button" data-oauth-provider={provider} class="w-full rounded-md border border-slate-300 px-4 py-2.5 text-sm font-semibold hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800">Continue with {provider}</button>
                ))}
              </div>
            )}
            {props.passkeys && (
              <button type="button" data-passkey-login class="w-full rounded-md bg-sky-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-sky-600">Sign in with a passkey</button>
            )}
            {props.email_otp && (
              <form data-email-otp class="space-y-3">
                <label class="block text-sm font-medium" for="auth-email">Email address</label>
                <input id="auth-email" name="email" type="email" autocomplete="email" required class="w-full rounded-md border border-slate-300 bg-transparent px-3 py-2 dark:border-slate-700" />
                <label data-otp-label hidden class="block text-sm font-medium" for="auth-code">Sign-in code</label>
                <input id="auth-code" data-otp-code hidden name="code" inputmode="numeric" autocomplete="one-time-code" class="w-full rounded-md border border-slate-300 bg-transparent px-3 py-2 dark:border-slate-700" />
                <button type="submit" class="w-full rounded-md bg-sky-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-sky-600">Send sign-in code</button>
              </form>
            )}
            {!(props.email_otp || props.passkeys || props.oauth_providers.length > 0) && <p class="text-sm text-amber-800 dark:text-amber-200">No sign-in method is configured.</p>}
            <p data-auth-status role="status" aria-live="polite" class="min-h-5 text-sm text-slate-600 dark:text-slate-400"></p>
          </div>
        ) : (
          <p class="mt-4 text-sm text-slate-600 dark:text-slate-400">Shared-server authentication is disabled. Return to the local workspace.</p>
        )}
      </section>
    </main>
  );
}
