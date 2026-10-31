import htmx, { type HtmxRequestConfig } from "htmx.org";
import { csrfFetch, csrfToken } from "./csrf";
import { cleanupSqlEditors, initializeSqlEditors, syncSqlForm } from "./islands/sql-editor";
import { cleanupErDiagrams, initializeErDiagrams } from "./islands/er-diagram";
import { initializeAuth } from "./islands/auth";

declare global {
  interface Window {
    csrfFetch: typeof csrfFetch;
    htmx: typeof htmx;
  }
}

window.htmx = htmx;
window.csrfFetch = csrfFetch;

document.addEventListener("submit", (event) => {
  const form = event.target;
  if (!(form instanceof HTMLFormElement)) return;
  const message = form.dataset.confirm;
  if (message && !window.confirm(message)) event.preventDefault();
});

document.addEventListener("htmx:configRequest", (event) => {
  const request = (event as CustomEvent<HtmxRequestConfig>).detail;
  request.headers["x-csrf-token"] = csrfToken();

  const form = request.elt instanceof HTMLFormElement
    ? request.elt
    : request.elt.closest("form");
  if (!form) return;

  const textarea = syncSqlForm(form);
  if (textarea?.name) request.parameters[textarea.name] = textarea.value;
});

document.addEventListener("htmx:beforeCleanupElement", (event) => {
  const element = (event as CustomEvent<{ elt: Element }>).detail.elt;
  if (element instanceof Element) cleanupSqlEditors(element);
  if (element instanceof Element) cleanupErDiagrams(element);
});

document.addEventListener("htmx:load", (event) => {
  const element = (event as CustomEvent<{ elt: Element }>).detail.elt;
  if (element instanceof Element) {
    initializeSqlEditors(element);
    initializeConnectionForms(element);
    initializeErDiagrams(element);
    initializeAuth(element);
  }
});

function initializeConnectionForms(root: ParentNode): void {
  const forms = root instanceof HTMLFormElement && root.matches("[data-connection-form]")
    ? [root]
    : Array.from(root.querySelectorAll<HTMLFormElement>("[data-connection-form]"));

  for (const form of forms) {
    if (form.dataset.engineBound) continue;
    const engine = form.querySelector<HTMLSelectElement>("[data-engine-select]");
    if (!engine) continue;
    const passwordFields = form.querySelector<HTMLFieldSetElement>("[data-password-fields]");
    form.dataset.engineBound = "true";

    const updateFields = (): void => {
      const isDsql = engine.value === "aurora_dsql";
      for (const element of form.querySelectorAll<HTMLElement>("[data-dsql-fields], [data-password-fields]")) {
        element.hidden = element.hasAttribute("data-dsql-fields") ? !isDsql : isDsql;
      }
      if (passwordFields) passwordFields.disabled = isDsql;
    };
    engine.addEventListener("change", () => {
      const selectedOption = engine.selectedOptions.item(0);
      const port = form.querySelector<HTMLInputElement>("[name=port]");
      if (port && selectedOption?.dataset.defaultPort) {
        port.value = selectedOption.dataset.defaultPort;
      }
      updateFields();
    });
    updateFields();
  }
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", () => {
    initializeSqlEditors(document);
    initializeConnectionForms(document);
    initializeErDiagrams(document);
    initializeAuth(document);
  }, { once: true });
} else {
  initializeSqlEditors(document);
  initializeConnectionForms(document);
  initializeErDiagrams(document);
  initializeAuth(document);
}

void import("htmx-ext-sse");
