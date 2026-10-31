import { csrfFetch } from "../csrf";

type JsonObject = Record<string, unknown>;
type AuthFlow = { flow_id: string; options: unknown };
type AuthResponse = {
  id: string;
  rawId: string;
  response: Record<string, unknown>;
  type: string;
};

function object(value: unknown): JsonObject | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as JsonObject
    : undefined;
}

function stringField(value: JsonObject, name: string): string {
  const field = value[name];
  if (typeof field !== "string") throw new Error(`Invalid authentication response: ${name}`);
  return field;
}

function errorCode(value: unknown): string | undefined {
  const candidate = object(value)?.code;
  return typeof candidate === "string" ? candidate : undefined;
}

async function authPost(path: string, payload?: unknown): Promise<unknown> {
  const hasPayload = payload !== undefined;
  const response = await csrfFetch(path, {
    method: "POST",
    ...(hasPayload
      ? { headers: { "content-type": "application/json" }, body: JSON.stringify(payload) }
      : {}),
  });
  const result: unknown = await response.json().catch(() => null);
  if (!response.ok) throw new Error(errorCode(result) ?? "Authentication failed");
  return result;
}

function parseFlow(value: unknown): AuthFlow {
  const result = object(value);
  if (!result || !("options" in result)) throw new Error("Invalid authentication flow");
  return { flow_id: stringField(result, "flow_id"), options: result.options };
}

function report(element: Element, message: string): void {
  const status = element.closest("[data-auth-status-root]")?.querySelector<HTMLElement>("[data-auth-status]")
    ?? element.closest("main")?.querySelector<HTMLElement>("[data-auth-status]");
  if (status) status.textContent = message;
}

function decodeBase64Url(value: string): ArrayBuffer {
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(base64 + "=".repeat((4 - base64.length % 4) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return bytes.buffer;
}

function encodeBase64Url(value: ArrayBuffer): string {
  const bytes = new Uint8Array(value);
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function credentialOptions(value: unknown): PublicKeyCredentialCreationOptions | PublicKeyCredentialRequestOptions {
  const root = object(value);
  if (!root) throw new Error("Invalid passkey options");
  const options = object(root.publicKey) ?? root;
  const converted: JsonObject = {
    ...options,
    challenge: decodeBase64Url(stringField(options, "challenge")),
  };
  if (options.user !== undefined) {
    const user = object(options.user);
    if (!user) throw new Error("Invalid passkey user options");
    converted.user = { ...user, id: decodeBase64Url(stringField(user, "id")) };
  }
  for (const name of ["allowCredentials", "excludeCredentials"]) {
    const list = options[name];
    if (list !== undefined) {
      if (!Array.isArray(list)) throw new Error("Invalid passkey credential options");
      converted[name] = list.map((item: unknown) => {
        const descriptor = object(item);
        if (!descriptor) throw new Error("Invalid passkey credential descriptor");
        return { ...descriptor, id: decodeBase64Url(stringField(descriptor, "id")) };
      });
    }
  }
  // Keep provider-specific WebAuthn fields after validating every binary field consumed by WebAuthn.
  return converted as unknown as PublicKeyCredentialCreationOptions | PublicKeyCredentialRequestOptions;
}

function credentialResponse(credential: PublicKeyCredential): AuthResponse {
  const response = credential.response;
  const serialized: Record<string, unknown> = {
    clientDataJSON: encodeBase64Url(response.clientDataJSON),
  };
  if (response instanceof AuthenticatorAttestationResponse) {
    serialized.attestationObject = encodeBase64Url(response.attestationObject);
    serialized.transports = response.getTransports();
  } else if (response instanceof AuthenticatorAssertionResponse) {
    serialized.authenticatorData = encodeBase64Url(response.authenticatorData);
    serialized.signature = encodeBase64Url(response.signature);
    serialized.userHandle = response.userHandle ? encodeBase64Url(response.userHandle) : null;
  }
  return {
    id: credential.id,
    rawId: encodeBase64Url(credential.rawId),
    response: serialized,
    type: credential.type,
  };
}

async function authenticateWithPasskey(button: HTMLButtonElement): Promise<void> {
  if (!navigator.credentials || !("PublicKeyCredential" in window)) throw new Error("Passkeys are not supported by this browser");
  report(button, "Waiting for your passkey…");
  const flow = parseFlow(await authPost("/auth/passkeys/authentication/start", { user: null }));
  const credential = await navigator.credentials.get({ publicKey: credentialOptions(flow.options) as PublicKeyCredentialRequestOptions });
  if (!(credential instanceof PublicKeyCredential)) throw new Error("The passkey ceremony was cancelled");
  await authPost("/auth/passkeys/authentication/finish", {
    flow_id: flow.flow_id,
    response: credentialResponse(credential),
  });
  window.location.assign("/");
}

async function registerPasskey(button: HTMLButtonElement): Promise<void> {
  if (!navigator.credentials || !("PublicKeyCredential" in window)) throw new Error("Passkeys are not supported by this browser");
  report(button, "Waiting for your passkey…");
  const flow = parseFlow(await authPost("/auth/passkeys/registration/start"));
  const credential = await navigator.credentials.create({ publicKey: credentialOptions(flow.options) as PublicKeyCredentialCreationOptions });
  if (!(credential instanceof PublicKeyCredential)) throw new Error("The passkey ceremony was cancelled");
  await authPost("/auth/passkeys/registration/finish", {
    flow_id: flow.flow_id,
    response: credentialResponse(credential),
  });
  report(button, "Passkey registered.");
  await loadPasskeys(button.closest("main") ?? document, true);
}

async function beginOAuth(button: HTMLButtonElement): Promise<void> {
  const provider = button.dataset.oauthProvider;
  if (!provider) return;
  report(button, "Connecting to sign-in provider…");
  const response = await csrfFetch(`/auth/oauth/${encodeURIComponent(provider)}/start`, { method: "POST" });
  const result: unknown = await response.json().catch(() => null);
  const authorizationUrl = object(result)?.authorization_url;
  if (!response.ok || typeof authorizationUrl !== "string") {
    throw new Error(errorCode(result) ?? "Could not start provider sign-in");
  }
  const destination = new URL(authorizationUrl, window.location.href);
  if (destination.protocol !== "https:") throw new Error("The provider returned an invalid sign-in URL");
  window.location.assign(destination.href);
}

async function submitEmailOtp(form: HTMLFormElement): Promise<void> {
  const data = new FormData(form);
  const email = String(data.get("email") ?? "");
  const code = String(data.get("code") ?? "");
  const button = form.querySelector<HTMLButtonElement>("button[type=submit]");
  if (!form.dataset.challengeId) {
    const result = object(await authPost("/auth/email-otp/request", { email }));
    const challengeId = result?.challenge_id;
    if (typeof challengeId !== "string") throw new Error("Could not start email sign-in");
    form.dataset.challengeId = challengeId;
    const label = form.querySelector<HTMLElement>("[data-otp-label]");
    const input = form.querySelector<HTMLInputElement>("[data-otp-code]");
    if (label) label.hidden = false;
    if (input) {
      input.hidden = false;
      input.required = true;
      input.focus();
    }
    if (button) button.textContent = "Verify sign-in code";
    report(form, "If the address can sign in, a code has been sent.");
    return;
  }
  await authPost("/auth/email-otp/verify", { challenge_id: form.dataset.challengeId, code });
  window.location.assign("/");
}

async function loadPasskeys(root: ParentNode, force = false): Promise<void> {
  const lists = root instanceof Element && root.matches("[data-passkey-list]")
    ? [root as HTMLUListElement]
    : Array.from(root.querySelectorAll<HTMLUListElement>("[data-passkey-list]"));
  for (const list of lists) {
    if (list.dataset.loaded && !force) continue;
    list.dataset.loaded = "true";
    try {
      const response = await fetch("/auth/passkeys", { credentials: "same-origin" });
      const result: unknown = await response.json().catch(() => null);
      const passkeys = object(result)?.passkeys;
      if (!response.ok || !Array.isArray(passkeys)) throw new Error("Could not load passkeys");
      list.replaceChildren();
      if (passkeys.length === 0) {
        const item = document.createElement("li");
        item.textContent = "No passkeys registered yet.";
        list.append(item);
      } else {
        for (const value of passkeys) {
          const passkey = object(value);
          const item = document.createElement("li");
          item.className = "break-all font-mono";
          item.textContent = passkey && typeof passkey.id === "string" ? passkey.id : "Unknown credential";
          list.append(item);
        }
      }
    } catch (error) {
      delete list.dataset.loaded;
      report(list, error instanceof Error ? error.message : "Could not load passkeys");
    }
  }
}

export function initializeAuth(root: ParentNode): void {
  void loadPasskeys(root);
}

document.addEventListener("click", (event) => {
  if (!(event.target instanceof Element)) return;
  const button = event.target.closest<HTMLButtonElement>("button[data-oauth-provider], button[data-passkey-login], button[data-passkey-register]");
  if (!button) return;
  event.preventDefault();
  const action = button.hasAttribute("data-oauth-provider")
    ? beginOAuth(button)
    : button.hasAttribute("data-passkey-login")
      ? authenticateWithPasskey(button)
      : registerPasskey(button);
  void action.catch((error: unknown) => report(button, error instanceof Error ? error.message : "Authentication failed"));
});

document.addEventListener("submit", (event) => {
  if (!(event.target instanceof HTMLFormElement)) return;
  const form = event.target;
  if (form.matches("form[data-auth-logout]")) {
    event.preventDefault();
    void csrfFetch(form.action, { method: "POST" })
      .then((response) => {
        if (!response.ok) throw new Error("Could not sign out");
        window.location.assign("/login");
      })
      .catch((error: unknown) => report(form, error instanceof Error ? error.message : "Could not sign out"));
    return;
  }
  if (!form.matches("form[data-email-otp]")) return;
  event.preventDefault();
  void submitEmailOtp(form).catch((error: unknown) => report(form, error instanceof Error ? error.message : "Authentication failed"));
});

