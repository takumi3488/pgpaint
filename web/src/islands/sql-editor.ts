import { basicSetup } from "codemirror";
import { PostgreSQL, sql } from "@codemirror/lang-sql";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { csrfFetch } from "../csrf";

interface CompletionSchema {
  default_schema: string;
  schemas: Record<string, Record<string, string[]>>;
}

interface SqlEditorMount {
  textarea: HTMLTextAreaElement;
  view: EditorView;
  mount: HTMLDivElement;
  schema: Compartment;
  theme: Compartment;
  colorScheme: MediaQueryList;
  colorSchemeChanged: (event: MediaQueryListEvent) => void;
  submitOnModEnter: (event: KeyboardEvent) => void;
}

const mounted = new WeakMap<HTMLTextAreaElement, SqlEditorMount>();
const liveMounts = new Set<SqlEditorMount>();

function themeExtension(matches: boolean): Extension {
  return matches ? oneDark : [];
}

function mountSqlEditor(textarea: HTMLTextAreaElement): void {
  if (mounted.has(textarea)) return;

  const schema = new Compartment();
  const theme = new Compartment();
  const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
  const readOnly = textarea.hasAttribute("data-readonly");
  const mount = document.createElement("div");
  mount.className = "pgpaint-sql-editor";
  mount.style.height = "15rem";
  mount.style.minHeight = "15rem";
  mount.style.resize = "vertical";
  mount.style.overflow = "auto";
  textarea.insertAdjacentElement("afterend", mount);

  const view = new EditorView({
    parent: mount,
    state: EditorState.create({
      doc: textarea.value,
      extensions: [
        basicSetup,
        schema.of(sql({ dialect: PostgreSQL })),
        theme.of(themeExtension(colorScheme.matches)),
        EditorState.readOnly.of(readOnly),
        EditorView.editable.of(!readOnly),
        EditorView.contentAttributes.of({ "aria-label": textarea.getAttribute("aria-label") ?? "SQL editor" }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) textarea.value = update.state.doc.toString();
        }),
      ],
    }),
  });
  view.dom.style.height = "100%";
  view.dom.style.minHeight = "15rem";

  const colorSchemeChanged = (event: MediaQueryListEvent) => {
    view.dispatch({ effects: theme.reconfigure(themeExtension(event.matches)) });
  };
  const submitOnModEnter = (event: KeyboardEvent) => {
    if (event.key !== "Enter" || (!event.metaKey && !event.ctrlKey)) return;
    event.preventDefault();
    event.stopPropagation();
    textarea.closest("form")?.requestSubmit();
  };
  mount.addEventListener("keydown", submitOnModEnter, true);
  const editor: SqlEditorMount = { textarea, view, mount, schema, theme, colorScheme, colorSchemeChanged, submitOnModEnter };
  mounted.set(textarea, editor);
  liveMounts.add(editor);
  colorScheme.addEventListener("change", colorSchemeChanged);
  void refreshSchema(editor);
}

async function refreshSchema(editor: SqlEditorMount): Promise<void> {
  const url = editor.textarea.dataset.completionUrl;
  if (!url || !editor.textarea.isConnected) return;

  try {
    const response = await csrfFetch(url, { headers: { Accept: "application/json" } });
    if (!response.ok) return;
    const completion = await response.json() as CompletionSchema;
    if (!editor.textarea.isConnected) return;
    editor.view.dispatch({
      effects: editor.schema.reconfigure(sql({
        dialect: PostgreSQL,
        schema: completion.schemas,
        defaultSchema: completion.default_schema,
      })),
    });
  } catch (error) {
    console.error("Failed to load SQL completion schema", error);
  }
}

export function initializeSqlEditors(root: ParentNode): void {
  if (root instanceof HTMLTextAreaElement && root.matches("[data-sql-editor]")) {
    mountSqlEditor(root);
  }
  root.querySelectorAll<HTMLTextAreaElement>("textarea[data-sql-editor]").forEach(mountSqlEditor);
}

export function syncSqlForm(form: HTMLFormElement): HTMLTextAreaElement | null {
  const textarea = form.querySelector<HTMLTextAreaElement>("textarea[data-sql-editor]");
  if (!textarea) return null;

  const editor = mounted.get(textarea);
  if (!editor) return textarea;
  const selection = editor.view.state.selection.main;
  textarea.value = selection.empty
    ? editor.view.state.doc.toString()
    : editor.view.state.sliceDoc(selection.from, selection.to);
  return textarea;
}

export function cleanupSqlEditors(root: Element): void {
  const textareas = root.matches("textarea[data-sql-editor]")
    ? [root as HTMLTextAreaElement]
    : root.querySelectorAll<HTMLTextAreaElement>("textarea[data-sql-editor]");
  textareas.forEach((textarea) => {
    const editor = mounted.get(textarea);
    if (!editor) return;
    editor.view.destroy();
    editor.mount.removeEventListener("keydown", editor.submitOnModEnter, true);
    editor.colorScheme.removeEventListener("change", editor.colorSchemeChanged);
    mounted.delete(textarea);
    liveMounts.delete(editor);
  });
}


window.addEventListener("pgpaint:schema-changed", () => {
  for (const editor of liveMounts) void refreshSchema(editor);
});
