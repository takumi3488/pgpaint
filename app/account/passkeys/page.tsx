export default function PasskeysPage() {
  return (
    <main class="mx-auto max-w-2xl px-5 py-10">
      <section class="rounded-2xl border border-stone-200 bg-white p-8 shadow-sm dark:border-stone-800 dark:bg-stone-900">
        <h1 class="text-2xl font-semibold tracking-tight">Passkeys</h1>
        <p class="mt-2 text-sm text-stone-600 dark:text-stone-400">Register a passkey on this device to sign in without an email code.</p>
        <button type="button" data-passkey-register class="mt-6 rounded-md bg-orange-700 px-4 py-2.5 text-sm font-semibold text-white hover:bg-orange-800">Register passkey</button>
        <p data-auth-status role="status" aria-live="polite" class="mt-4 min-h-5 text-sm text-stone-600 dark:text-stone-400"></p>
        <h2 class="mt-8 text-lg font-semibold">Registered credentials</h2>
        <ul data-passkey-list class="mt-3 space-y-2 text-sm text-stone-600 dark:text-stone-400"></ul>
      </section>
    </main>
  );
}
