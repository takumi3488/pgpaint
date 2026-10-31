interface GraphColumn {
  name: string;
  data_type: string;
  pk: boolean;
  fk: boolean;
  nullable: boolean;
}

interface GraphTable {
  name: string;
  columns: GraphColumn[];
}

interface GraphEdge {
  from_table: string;
  from_columns: string[];
  to_table: string;
  to_columns: string[];
  inferred: boolean;
  cardinality: string;
}

interface GraphResponse {
  tables: GraphTable[];
  edges: GraphEdge[];
  total_tables: number;
}

interface Point {
  x: number;
  y: number;
}

interface Box extends Point {
  width: number;
  height: number;
  table: GraphTable;
}

interface ViewBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface Palette {
  canvas: string;
  table: string;
  header: string;
  border: string;
  text: string;
  muted: string;
  rule: string;
  edge: string;
  inferred: string;
  pk: string;
  fk: string;
  highlight: string;
}

interface DiagramMount {
  canvas: HTMLElement;
  graphUrl: string;
  tableUrlPrefix: string;
  schema: string;
  search: HTMLInputElement;
  select: HTMLSelectElement;
  status: HTMLElement;
  listeners: AbortController;
  colorScheme: MediaQueryList;
  colorSchemeChanged: () => void;
  resizeObserver: ResizeObserver;
  svg: SVGSVGElement | null;
  graph: GraphResponse | null;
  baseViewBox: ViewBox | null;
  viewBox: ViewBox | null;
  request: AbortController | null;
  searchTimer: number | null;
  drag: { x: number; y: number; viewBox: ViewBox } | null;
}

const mounted = new WeakMap<HTMLElement, DiagramMount>();
const svgNamespace = "http://www.w3.org/2000/svg";
const nodeWidth = 300;
const columnHeight = 27;
const headerHeight = 42;
const horizontalGap = 150;
const verticalGap = 54;
const diagramPadding = 72;
let nextSvgId = 1;

function createSvg<K extends keyof SVGElementTagNameMap>(tag: K): SVGElementTagNameMap[K] {
  return document.createElementNS(svgNamespace, tag);
}

function palette(dark: boolean): Palette {
  return dark
    ? {
        canvas: "#020617",
        table: "#0f172a",
        header: "#1e293b",
        border: "#475569",
        text: "#e2e8f0",
        muted: "#94a3b8",
        rule: "#334155",
        edge: "#94a3b8",
        inferred: "#c4b5fd",
        pk: "#fbbf24",
        fk: "#38bdf8",
        highlight: "#1e3a8a",
      }
    : {
        canvas: "#ffffff",
        table: "#ffffff",
        header: "#f1f5f9",
        border: "#94a3b8",
        text: "#0f172a",
        muted: "#475569",
        rule: "#e2e8f0",
        edge: "#64748b",
        inferred: "#7c3aed",
        pk: "#b45309",
        fk: "#0369a1",
        highlight: "#dbeafe",
      };
}

function appendText(parent: SVGElement, value: string, x: number, y: number, color: string, size = 12): SVGTextElement {
  const text = createSvg("text");
  text.setAttribute("x", String(x));
  text.setAttribute("y", String(y));
  text.setAttribute("fill", color);
  text.setAttribute("font-size", String(size));
  text.setAttribute("font-family", "ui-sans-serif, system-ui, sans-serif");
  text.textContent = value;
  parent.append(text);
  return text;
}

function layoutTables(graph: GraphResponse): { boxes: Box[]; width: number; height: number } {
  const names = graph.tables.map((table) => table.name);
  const rank = new Map(names.map((name) => [name, 0]));
  const children = new Map(names.map((name) => [name, [] as string[]]));
  const inDegree = new Map(names.map((name) => [name, 0]));
  for (const edge of graph.edges) {
    if (!rank.has(edge.from_table) || !rank.has(edge.to_table)) continue;
    children.get(edge.to_table)?.push(edge.from_table);
    inDegree.set(edge.from_table, (inDegree.get(edge.from_table) ?? 0) + 1);
  }

  const queue = names.filter((name) => inDegree.get(name) === 0).sort();
  const visited = new Set<string>();
  while (queue.length > 0) {
    const parent = queue.shift();
    if (!parent) continue;
    visited.add(parent);
    for (const child of children.get(parent) ?? []) {
      rank.set(child, Math.max(rank.get(child) ?? 0, (rank.get(parent) ?? 0) + 1));
      const degree = (inDegree.get(child) ?? 0) - 1;
      inDegree.set(child, degree);
      if (degree === 0) queue.push(child);
    }
  }
  for (const name of names) {
    if (!visited.has(name)) rank.set(name, 0);
  }

  const layers: GraphTable[][] = [];
  for (const table of graph.tables) {
    const layer = rank.get(table.name) ?? 0;
    layers[layer] ??= [];
    layers[layer].push(table);
  }
  layers.forEach((layer) => layer?.sort((left, right) => left.name.localeCompare(right.name)));

  const layerWidths = layers.map((layer) => layer?.length ? nodeWidth : 0);
  let x = diagramPadding;
  const boxes: Box[] = [];
  let contentHeight = 0;
  layers.forEach((layer, layerIndex) => {
    if (!layer) return;
    let y = diagramPadding;
    for (const table of layer) {
      const height = headerHeight + Math.max(table.columns.length, 1) * columnHeight + 12;
      boxes.push({ x, y, width: nodeWidth, height, table });
      y += height + verticalGap;
    }
    contentHeight = Math.max(contentHeight, y - verticalGap + diagramPadding);
    x += (layerWidths[layerIndex] ?? nodeWidth) + horizontalGap;
  });

  return {
    boxes,
    width: Math.max(x - horizontalGap + diagramPadding, 900),
    height: Math.max(contentHeight, 500),
  };
}

function drawGraph(mount: DiagramMount): void {
  const graph = mount.graph;
  if (!graph) return;
  const { boxes, width, height } = layoutTables(graph);
  const colors = palette(mount.colorScheme.matches);
  const svg = createSvg("svg");
  svg.setAttribute("xmlns", svgNamespace);
  svg.setAttribute("width", "100%");
  svg.setAttribute("height", "100%");
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("role", "group");
  svg.setAttribute("aria-label", `Entity relationship diagram for ${mount.schema}`);
  svg.style.display = "block";
  svg.style.background = colors.canvas;

  const background = createSvg("rect");
  background.setAttribute("x", "0");
  background.setAttribute("y", "0");
  background.setAttribute("width", String(width));
  background.setAttribute("height", String(height));
  background.setAttribute("fill", colors.canvas);
  svg.append(background);

  const markerId = `er-arrow-${nextSvgId++}`;
  const defs = createSvg("defs");
  const marker = createSvg("marker");
  marker.setAttribute("id", markerId);
  marker.setAttribute("viewBox", "0 0 10 10");
  marker.setAttribute("refX", "8");
  marker.setAttribute("refY", "5");
  marker.setAttribute("markerWidth", "7");
  marker.setAttribute("markerHeight", "7");
  marker.setAttribute("orient", "auto-start-reverse");
  const arrow = createSvg("path");
  arrow.setAttribute("d", "M 0 0 L 10 5 L 0 10 z");
  arrow.setAttribute("fill", colors.edge);
  marker.append(arrow);
  defs.append(marker);
  svg.append(defs);

  const byName = new Map(boxes.map((box) => [box.table.name, box]));
  const search = mount.search.value.trim().toLocaleLowerCase();
  for (const edge of graph.edges) {
    const from = byName.get(edge.from_table);
    const to = byName.get(edge.to_table);
    if (!from || !to) continue;
    const fromCenter = from.y + from.height / 2;
    const toCenter = to.y + to.height / 2;
    const sameLayer = Math.abs(from.x - to.x) < 1;
    const forward = to.x >= from.x + from.width;
    const sourceX = sameLayer || forward ? from.x + from.width : from.x;
    const targetX = sameLayer ? to.x + to.width : forward ? to.x : to.x + to.width;
    const middleX = sameLayer ? sourceX + 32 : (sourceX + targetX) / 2;
    const stroke = edge.inferred ? colors.inferred : colors.edge;
    const path = createSvg("path");
    path.setAttribute("d", `M ${sourceX} ${fromCenter} H ${middleX} V ${toCenter} H ${targetX}`);
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", stroke);
    path.setAttribute("stroke-width", "2");
    path.setAttribute("marker-end", `url(#${markerId})`);
    if (edge.inferred) path.setAttribute("stroke-dasharray", "7 5");
    const title = createSvg("title");
    title.textContent = `${edge.from_table} (${edge.from_columns.join(", ")}) → ${edge.to_table} (${edge.to_columns.join(", ")}); ${edge.cardinality}${edge.inferred ? "; inferred" : ""}`;
    path.append(title);
    svg.append(path);

    const label = appendText(svg, edge.cardinality, middleX + 5, (fromCenter + toCenter) / 2 - 4, stroke, 11);
    label.setAttribute("font-weight", "600");
  }

  for (const box of boxes) {
    const group = createSvg("g");
    group.setAttribute("transform", `translate(${box.x} ${box.y})`);
    group.setAttribute("role", "link");
    group.setAttribute("tabindex", "0");
    group.setAttribute("aria-label", `Open table ${mount.schema}.${box.table.name}`);
    group.setAttribute("data-table-name", box.table.name);
    group.style.cursor = "pointer";
    const highlighted = search.length > 0 && box.table.name.toLocaleLowerCase().includes(search);

    const card = createSvg("rect");
    card.setAttribute("width", String(box.width));
    card.setAttribute("height", String(box.height));
    card.setAttribute("rx", "7");
    card.setAttribute("fill", highlighted ? colors.highlight : colors.table);
    card.setAttribute("stroke", highlighted ? colors.fk : colors.border);
    card.setAttribute("stroke-width", highlighted ? "2.5" : "1.5");
    group.append(card);

    const header = createSvg("path");
    header.setAttribute("d", `M 7 0 H ${box.width - 7} Q ${box.width} 0 ${box.width} 7 V ${headerHeight} H 0 V 7 Q 0 0 7 0`);
    header.setAttribute("fill", colors.header);
    group.append(header);
    appendText(group, box.table.name, 12, 27, colors.text, 14).setAttribute("font-weight", "700");

    box.table.columns.forEach((column, index) => {
      const y = headerHeight + 13 + index * columnHeight;
      const rule = createSvg("line");
      rule.setAttribute("x1", "0");
      rule.setAttribute("x2", String(box.width));
      rule.setAttribute("y1", String(y + 7));
      rule.setAttribute("y2", String(y + 7));
      rule.setAttribute("stroke", colors.rule);
      rule.setAttribute("stroke-width", "1");
      group.append(rule);

      let markerX = 11;
      if (column.pk) {
        appendText(group, "PK", markerX, y, colors.pk, 10).setAttribute("font-weight", "700");
        markerX += 23;
      }
      if (column.fk) {
        appendText(group, "FK", markerX, y, colors.fk, 10).setAttribute("font-weight", "700");
        markerX += 23;
      }
      const availableWidth = Math.max(60, box.width - markerX - 66);
      const columnLabel = `${column.name}  ${column.data_type}`;
      const visibleLabel = columnLabel.length > 34 ? `${columnLabel.slice(0, 31)}…` : columnLabel;
      const name = appendText(group, visibleLabel, markerX, y, colors.text, 11);
      name.setAttribute("title", columnLabel);
      appendText(group, column.nullable ? "NULL" : "NOT NULL", box.width - 57, y, colors.muted, 9);
      if (visibleLabel.length * 6 > availableWidth) name.setAttribute("font-size", "10");
    });

    const title = createSvg("title");
    title.textContent = `Open ${mount.schema}.${box.table.name}`;
    group.append(title);
    const open = () => {
      window.location.assign(`${mount.tableUrlPrefix}${encodeURIComponent(mount.schema)}/${encodeURIComponent(box.table.name)}`);
    };
    group.addEventListener("click", open, { signal: mount.listeners.signal });
    group.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        open();
      }
    }, { signal: mount.listeners.signal });
    svg.append(group);
  }

  mount.svg = svg;
  mount.baseViewBox = { x: 0, y: 0, width, height };
  mount.viewBox = { ...mount.baseViewBox };
  mount.canvas.replaceChildren(svg);
  fitToView(mount);
}

function setViewBox(mount: DiagramMount, viewBox: ViewBox): void {
  if (!mount.svg) return;
  mount.viewBox = viewBox;
  mount.svg.setAttribute("viewBox", `${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`);
}

function fitToView(mount: DiagramMount): void {
  if (mount.baseViewBox) setViewBox(mount, { ...mount.baseViewBox });
}

function download(filename: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function exportSvg(mount: DiagramMount): void {
  if (!mount.svg || !mount.baseViewBox) return;
  const copy = mount.svg.cloneNode(true) as SVGSVGElement;
  copy.setAttribute("width", String(mount.baseViewBox.width));
  copy.setAttribute("height", String(mount.baseViewBox.height));
  copy.setAttribute("viewBox", `0 0 ${mount.baseViewBox.width} ${mount.baseViewBox.height}`);
  const content = new XMLSerializer().serializeToString(copy);
  download(`er-${safeFilename(mount.schema)}.svg`, new Blob([content], { type: "image/svg+xml;charset=utf-8" }));
}

function exportPng(mount: DiagramMount): void {
  if (!mount.svg || !mount.baseViewBox) return;
  const copy = mount.svg.cloneNode(true) as SVGSVGElement;
  const { width, height } = mount.baseViewBox;
  copy.setAttribute("width", String(width));
  copy.setAttribute("height", String(height));
  copy.setAttribute("viewBox", `0 0 ${width} ${height}`);
  const source = URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(copy)], { type: "image/svg+xml;charset=utf-8" }));
  const image = new Image();
  image.onload = () => {
    const canvas = document.createElement("canvas");
    const scale = Math.min(1, 16_000 / Math.max(width, height));
    canvas.width = Math.ceil(width * scale);
    canvas.height = Math.ceil(height * scale);
    const context = canvas.getContext("2d");
    if (!context) {
      URL.revokeObjectURL(source);
      mount.status.textContent = "PNG export is unavailable in this browser.";
      return;
    }
    canvas.toBlob((blob) => {
      if (blob) download(`er-${safeFilename(mount.schema)}.png`, blob);
      else mount.status.textContent = "The diagram is too large to export as PNG.";
      URL.revokeObjectURL(source);
    }, "image/png");
  };
  image.onerror = () => {
    URL.revokeObjectURL(source);
    mount.status.textContent = "Could not export the diagram as PNG.";
  };
  image.src = source;
}

function safeFilename(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, "-") || "schema";
}

async function loadGraph(mount: DiagramMount, filter: string): Promise<void> {
  mount.request?.abort();
  const request = new AbortController();
  mount.request = request;
  const url = new URL(mount.graphUrl, window.location.origin);
  if (filter) url.searchParams.set("filter", filter);
  mount.status.textContent = "Loading diagram…";
  try {
    const response = await fetch(url, { headers: { accept: "application/json" }, signal: request.signal });
    if (!response.ok) throw new Error(`Diagram request failed (${response.status}).`);
    const graph = await response.json() as GraphResponse;
    if (mount.request !== request) return;
    mount.graph = graph;
    drawGraph(mount);
    if (graph.total_tables > graph.tables.length) {
      mount.status.textContent = `Showing ${graph.tables.length} of ${graph.total_tables} tables. Filter to find other tables.`;
    } else if (graph.tables.length === 0) {
      mount.status.textContent = filter ? `No tables match “${filter}”.` : "This schema has no tables to display.";
    } else {
      mount.status.textContent = `${graph.tables.length} ${graph.tables.length === 1 ? "table" : "tables"}${filter ? ` matching “${filter}”` : ""}.`;
    }
  } catch (error) {
    if (request.signal.aborted) return;
    mount.status.textContent = error instanceof Error ? error.message : "Could not load the ER diagram.";
  }
}

function mountDiagram(root: HTMLElement): void {
  if (mounted.has(root)) return;
  const canvas = root.querySelector<HTMLElement>("[data-er-canvas]");
  const search = root.querySelector<HTMLInputElement>("[data-er-search]");
  const select = root.querySelector<HTMLSelectElement>("[data-er-schema-select]");
  const status = root.querySelector<HTMLElement>("[data-er-status]");
  const fitButton = root.querySelector<HTMLButtonElement>("[data-er-fit]");
  const exportSvgButton = root.querySelector<HTMLButtonElement>("[data-er-export-svg]");
  const exportPngButton = root.querySelector<HTMLButtonElement>("[data-er-export-png]");
  const graphUrl = root.dataset.graphUrl;
  const tableUrlPrefix = root.dataset.tableUrlPrefix;
  const schema = root.dataset.schema;
  if (!canvas || !search || !select || !status || !fitButton || !exportSvgButton || !exportPngButton || !graphUrl || !tableUrlPrefix || !schema) return;

  const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
  const mount: DiagramMount = {
    canvas,
    graphUrl,
    tableUrlPrefix,
    schema,
    search,
    select,
    status,
    listeners: new AbortController(),
    colorScheme,
    colorSchemeChanged: () => {
      const viewBox = mount.viewBox ? { ...mount.viewBox } : null;
      drawGraph(mount);
      if (viewBox) setViewBox(mount, viewBox);
    },
    resizeObserver: new ResizeObserver(() => fitToView(mount)),
    svg: null,
    graph: null,
    baseViewBox: null,
    viewBox: null,
    request: null,
    searchTimer: null,
    drag: null,
  };
  mounted.set(root, mount);
  const signal = mount.listeners.signal;

  select.addEventListener("change", () => window.location.assign(select.value), { signal });
  search.addEventListener("input", () => {
    if (mount.searchTimer !== null) window.clearTimeout(mount.searchTimer);
    mount.searchTimer = window.setTimeout(() => {
      mount.searchTimer = null;
      void loadGraph(mount, search.value.trim());
    }, 220);
  }, { signal });
  fitButton.addEventListener("click", () => fitToView(mount), { signal });
  exportSvgButton.addEventListener("click", () => exportSvg(mount), { signal });
  exportPngButton.addEventListener("click", () => exportPng(mount), { signal });
  canvas.addEventListener("wheel", (event) => {
    if (!mount.svg || !mount.viewBox) return;
    event.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const ratioX = (event.clientX - rect.left) / rect.width;
    const ratioY = (event.clientY - rect.top) / rect.height;
    if (!mount.baseViewBox) return;
    const factor = event.deltaY < 0 ? 0.88 : 1.12;
    const viewBox = mount.viewBox;
    const zoom = Math.min(12, Math.max(0.08, viewBox.width / mount.baseViewBox.width * factor));
    const width = mount.baseViewBox.width * zoom;
    const height = mount.baseViewBox.height * zoom;
    setViewBox(mount, {
      x: viewBox.x + (viewBox.width - width) * ratioX,
      y: viewBox.y + (viewBox.height - height) * ratioY,
      width,
      height,
    });
  }, { passive: false, signal });
  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || !mount.viewBox) return;
    const target = event.target;
    if (target instanceof Element && target.closest("[data-table-name]")) return;
    mount.drag = { x: event.clientX, y: event.clientY, viewBox: { ...mount.viewBox } };
    canvas.setPointerCapture(event.pointerId);
  }, { signal });
  canvas.addEventListener("pointermove", (event) => {
    if (!mount.drag || !mount.viewBox) return;
    const rect = canvas.getBoundingClientRect();
    const deltaX = event.clientX - mount.drag.x;
    const deltaY = event.clientY - mount.drag.y;
    setViewBox(mount, {
      ...mount.drag.viewBox,
      x: mount.drag.viewBox.x - deltaX * mount.drag.viewBox.width / rect.width,
      y: mount.drag.viewBox.y - deltaY * mount.drag.viewBox.height / rect.height,
    });
  }, { signal });
  const endDrag = () => { mount.drag = null; };
  canvas.addEventListener("pointerup", endDrag, { signal });
  canvas.addEventListener("pointercancel", endDrag, { signal });
  colorScheme.addEventListener("change", mount.colorSchemeChanged);
  mount.resizeObserver.observe(canvas);
  void loadGraph(mount, "");
}

export function initializeErDiagrams(root: ParentNode): void {
  if (root instanceof HTMLElement && root.matches("[data-er-diagram]")) mountDiagram(root);
  root.querySelectorAll<HTMLElement>("[data-er-diagram]").forEach(mountDiagram);
}

export function cleanupErDiagrams(root: Element): void {
  const roots = root.matches("[data-er-diagram]")
    ? [root as HTMLElement]
    : root.querySelectorAll<HTMLElement>("[data-er-diagram]");
  roots.forEach((element) => {
    const mount = mounted.get(element);
    if (!mount) return;
    mount.listeners.abort();
    mount.request?.abort();
    if (mount.searchTimer !== null) window.clearTimeout(mount.searchTimer);
    mount.colorScheme.removeEventListener("change", mount.colorSchemeChanged);
    mount.resizeObserver.disconnect();
    mounted.delete(element);
  });
}
