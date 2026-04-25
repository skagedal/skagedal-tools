// Browser-side log viewer. Fetches config + entries via SSE and renders a
// simple table with vi-like navigation (j/k/u/o, g/G, esc, v/t).
//
// State model:
//   fields: ManagedField[] — every column we know about. visible == false
//                            means it's not rendered as a table column but
//                            still listed in the v menu.
//   entries: LogEntry[]    — log entries in arrival order.
//   cursor: number         — selected entry index in the list.
//   expanded: Set<number>  — entry ids whose detail view is shown inline.
//   detailCursor: number   — which row of the expanded detail is selected
//                            (0 = the entry header itself, 1..n = fields).

const headerRow = document.getElementById("header-row");
const rowsEl = document.getElementById("rows");
const statusEl = document.getElementById("status");
const metaEl = document.getElementById("meta");
const fieldsBtn = document.getElementById("fields-btn");
const fieldsPopover = document.getElementById("fields-popover");
const fieldsList = document.getElementById("fields-list");
const fieldsClose = document.getElementById("fields-close");

let sourceLabel = "";
let fields = [];
let entries = [];
let cursor = 0;
let expanded = new Set();
let detailCursor = 0;
let done = false;
let follow = false;

async function start() {
  const meta = await fetch("/api/meta").then((r) => r.json());
  sourceLabel = meta.sourceLabel;
  fields = (meta.config.fields ?? []).map((f) => ({
    name: f.name,
    from: [...f.from],
    visible: true,
  }));
  metaEl.textContent = `· ${sourceLabel}`;
  renderHeader();
  renderRows();
  startStream();
  wireFieldsPopover();
}

function renderHeader() {
  headerRow.replaceChildren();
  const visible = fields.filter((f) => f.visible);
  const last = visible.length - 1;
  for (let i = 0; i < visible.length; i++) {
    const field = visible[i];
    const th = document.createElement("th");
    th.textContent = field.name;
    th.classList.add(i === last ? "col-flex" : "col-fit");
    headerRow.appendChild(th);
  }
  if (visible.length === 0) {
    const th = document.createElement("th");
    th.textContent = "(no visible fields — open the Fields menu)";
    th.classList.add("col-flex");
    headerRow.appendChild(th);
  }
}

function startStream() {
  const es = new EventSource("/api/stream");
  es.addEventListener("entry", (ev) => {
    const entry = JSON.parse(ev.data);
    entries.push(entry);
    mergeSeenKeys(Object.keys(entry.data));
    renderRows();
    updateStatus();
    if (follow) {
      setCursor(entries.length - 1, { keepFollow: true });
    } else if (entries.length === 1) {
      setCursor(0);
    }
  });
  es.addEventListener("end", () => {
    done = true;
    updateStatus();
    es.close();
  });
  es.onerror = () => {
    statusEl.textContent = "(connection lost)";
  };
}

function mergeSeenKeys(keys) {
  const known = new Set();
  for (const f of fields) for (const k of f.from) known.add(k);
  let added = false;
  for (const key of keys) {
    if (known.has(key)) continue;
    known.add(key);
    fields.push({ name: key, from: [key], visible: false });
    added = true;
  }
  if (added && !fieldsPopover.hidden) renderFieldsList();
}

function renderRows() {
  rowsEl.replaceChildren();
  const visible = fields.filter((f) => f.visible);
  const colCount = Math.max(1, visible.length);
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    const tr = document.createElement("tr");
    tr.dataset.id = String(entry.id);
    if (i === cursor && !follow) tr.classList.add("selected");
    const last = visible.length - 1;
    for (let j = 0; j < visible.length; j++) {
      const field = visible[j];
      const td = document.createElement("td");
      const value = pickField(entry.data, field.from);
      td.textContent = value;
      td.classList.add(j === last ? "col-flex" : "col-fit");
      if (field.name.toLowerCase() === "level") {
        td.classList.add(`level-${classifyLevel(value)}`);
      }
      tr.appendChild(td);
    }
    if (visible.length === 0) {
      const td = document.createElement("td");
      td.textContent = "—";
      td.classList.add("col-flex");
      tr.appendChild(td);
    }
    tr.addEventListener("click", () => {
      setCursor(i);
      toggleExpand(entry.id);
    });
    rowsEl.appendChild(tr);
    if (expanded.has(entry.id)) {
      rowsEl.appendChild(renderDetailRow(entry, colCount, i === cursor));
    }
  }
  if (cursor >= 0 && cursor < entries.length) {
    const sel = rowsEl.querySelector(`tr.selected`);
    if (sel && follow === false) sel.scrollIntoView({ block: "nearest" });
  }
}

function renderDetailRow(entry, colCount, parentSelected) {
  const tr = document.createElement("tr");
  tr.classList.add("detail-row");
  const td = document.createElement("td");
  td.colSpan = colCount;
  td.appendChild(renderDetailContent(entry, parentSelected));
  tr.appendChild(td);
  return tr;
}

function renderDetailContent(entry, parentSelected) {
  const wrap = document.createElement("div");
  wrap.classList.add("detail");

  const head = document.createElement("div");
  head.classList.add("detail-head");
  if (parentSelected && detailCursor === 0) head.classList.add("detail-selected");
  const title = document.createElement("strong");
  title.textContent = `entry #${entry.id}${entry.wrapped ? " (wrapped)" : ""}`;
  head.appendChild(title);
  const copyEntry = document.createElement("button");
  copyEntry.type = "button";
  copyEntry.textContent = "Copy JSON";
  copyEntry.addEventListener("click", (ev) => {
    ev.stopPropagation();
    copyText(JSON.stringify(entry.data, null, 2), `entry #${entry.id}`);
  });
  head.appendChild(copyEntry);
  head.addEventListener("click", () => {
    if (parentSelected) {
      detailCursor = 0;
      renderRows();
    }
  });
  wrap.appendChild(head);

  const keys = Object.keys(entry.data);
  if (keys.length === 0) {
    const empty = document.createElement("div");
    empty.classList.add("detail-empty");
    empty.textContent = "(empty)";
    wrap.appendChild(empty);
    return wrap;
  }

  const list = document.createElement("dl");
  list.classList.add("detail-fields");
  keys.forEach((key, idx) => {
    const row = document.createElement("div");
    row.classList.add("detail-field");
    if (parentSelected && detailCursor === idx + 1) row.classList.add("detail-selected");
    const visible = isKeyVisible(key);
    if (!visible) row.classList.add("hidden-field");

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = visible;
    checkbox.title = visible ? "Hide this column" : "Show this column";
    checkbox.addEventListener("click", (ev) => ev.stopPropagation());
    checkbox.addEventListener("change", () => {
      toggleByKey(key);
      renderHeader();
      renderRows();
      if (!fieldsPopover.hidden) renderFieldsList();
    });
    row.appendChild(checkbox);

    const dt = document.createElement("dt");
    dt.textContent = key;
    row.appendChild(dt);

    const dd = document.createElement("dd");
    dd.textContent = renderValueDetailed(entry.data[key]);
    row.appendChild(dd);

    const copyBtn = document.createElement("button");
    copyBtn.type = "button";
    copyBtn.textContent = "Copy";
    copyBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      copyText(stringifyValue(entry.data[key]), key);
    });
    row.appendChild(copyBtn);

    row.addEventListener("click", () => {
      if (parentSelected) {
        detailCursor = idx + 1;
        renderRows();
      }
    });

    list.appendChild(row);
  });
  wrap.appendChild(list);
  return wrap;
}

function pickField(data, candidates) {
  for (const key of candidates) {
    const value = data[key];
    if (value === undefined || value === null || value === "") continue;
    return typeof value === "string" ? value : JSON.stringify(value);
  }
  return "";
}

function stringifyValue(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function renderValueDetailed(value) {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function classifyLevel(level) {
  const v = String(level).toLowerCase();
  if (v.includes("error") || v === "err" || v === "fatal") return "error";
  if (v.includes("warn")) return "warn";
  if (v.includes("info")) return "info";
  if (v.includes("debug") || v.includes("trace")) return "debug";
  return "other";
}

function setCursor(i, opts = {}) {
  if (entries.length === 0) return;
  if (!opts.keepFollow && follow) follow = false;
  cursor = Math.max(0, Math.min(entries.length - 1, i));
  detailCursor = 0;
  renderRows();
  updateStatus();
}

function toggleExpand(id) {
  if (expanded.has(id)) expanded.delete(id);
  else expanded.add(id);
  detailCursor = 0;
  renderRows();
}

function toggleFollow() {
  follow = !follow;
  if (follow && entries.length > 0) {
    cursor = entries.length - 1;
  }
  renderRows();
  updateStatus();
}

function updateStatus() {
  const pos = entries.length === 0 ? 0 : cursor + 1;
  const eof = done ? " (eof)" : "";
  const tag = follow ? " · FOLLOW" : "";
  statusEl.textContent = `${pos}/${entries.length}${eof}${tag}`;
}

function isKeyVisible(key) {
  const f = fields.find((f) => f.from.includes(key));
  return f ? f.visible : false;
}

function toggleByKey(key) {
  const idx = fields.findIndex((f) => f.from.includes(key));
  if (idx === -1) {
    fields.push({ name: key, from: [key], visible: true });
  } else {
    fields[idx] = { ...fields[idx], visible: !fields[idx].visible };
  }
}

function copyText(text, label) {
  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(text).then(
      () => flashStatus(`copied ${label}`),
      () => flashStatus(`copy failed`),
    );
  } else {
    flashStatus("clipboard unavailable");
  }
}

let flashTimer = null;
function flashStatus(msg) {
  const original = statusEl.textContent;
  statusEl.textContent = `${original} · ${msg}`;
  if (flashTimer) clearTimeout(flashTimer);
  flashTimer = setTimeout(() => updateStatus(), 1500);
}

function wireFieldsPopover() {
  fieldsBtn.addEventListener("click", () => {
    if (fieldsPopover.hidden) openFields();
    else closeFields();
  });
  fieldsClose.addEventListener("click", closeFields);
  document.addEventListener("click", (ev) => {
    if (fieldsPopover.hidden) return;
    if (fieldsPopover.contains(ev.target) || fieldsBtn.contains(ev.target)) return;
    closeFields();
  });
}

function openFields() {
  fieldsPopover.hidden = false;
  fieldsBtn.setAttribute("aria-expanded", "true");
  renderFieldsList();
}

function closeFields() {
  fieldsPopover.hidden = true;
  fieldsBtn.setAttribute("aria-expanded", "false");
}

function renderFieldsList() {
  fieldsList.replaceChildren();
  fields.forEach((field, i) => {
    const li = document.createElement("li");
    li.draggable = true;
    li.dataset.index = String(i);

    const handle = document.createElement("span");
    handle.className = "drag-handle";
    handle.textContent = "≡";
    li.appendChild(handle);

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = field.visible;
    checkbox.addEventListener("change", () => {
      fields[i] = { ...fields[i], visible: checkbox.checked };
      renderHeader();
      renderRows();
      renderFieldsList();
    });
    li.appendChild(checkbox);

    const label = document.createElement("span");
    label.className = "field-name";
    label.textContent = field.name;
    li.appendChild(label);

    if (field.from.length > 1 || field.from[0] !== field.name) {
      const hint = document.createElement("span");
      hint.className = "field-from";
      hint.textContent = `(from: ${field.from.join(", ")})`;
      li.appendChild(hint);
    }

    const up = document.createElement("button");
    up.type = "button";
    up.textContent = "↑";
    up.title = "Move up";
    up.disabled = i === 0;
    up.addEventListener("click", () => moveField(i, -1));
    li.appendChild(up);

    const down = document.createElement("button");
    down.type = "button";
    down.textContent = "↓";
    down.title = "Move down";
    down.disabled = i === fields.length - 1;
    down.addEventListener("click", () => moveField(i, +1));
    li.appendChild(down);

    li.addEventListener("dragstart", (ev) => {
      ev.dataTransfer.effectAllowed = "move";
      ev.dataTransfer.setData("text/plain", String(i));
      li.classList.add("dragging");
    });
    li.addEventListener("dragend", () => li.classList.remove("dragging"));
    li.addEventListener("dragover", (ev) => {
      ev.preventDefault();
      ev.dataTransfer.dropEffect = "move";
    });
    li.addEventListener("drop", (ev) => {
      ev.preventDefault();
      const from = Number(ev.dataTransfer.getData("text/plain"));
      const to = i;
      if (Number.isFinite(from) && from !== to) reorderField(from, to);
    });

    fieldsList.appendChild(li);
  });
  if (fields.length === 0) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = "(no fields seen yet)";
    fieldsList.appendChild(empty);
  }
}

function moveField(index, delta) {
  const target = index + delta;
  if (target < 0 || target >= fields.length) return;
  const [item] = fields.splice(index, 1);
  fields.splice(target, 0, item);
  renderHeader();
  renderRows();
  renderFieldsList();
}

function reorderField(from, to) {
  const [item] = fields.splice(from, 1);
  fields.splice(to, 0, item);
  renderHeader();
  renderRows();
  renderFieldsList();
}

document.addEventListener("keydown", (ev) => {
  if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
  const target = ev.target;
  if (target && (target.tagName === "INPUT" || target.tagName === "BUTTON")) {
    if (ev.key === "Escape") (target).blur?.();
    return;
  }
  if (!fieldsPopover.hidden) {
    if (ev.key === "Escape" || ev.key === "v" || ev.key === "q") {
      closeFields();
      ev.preventDefault();
    }
    return;
  }
  const entry = entries[cursor];
  const isExpanded = entry && expanded.has(entry.id);
  switch (ev.key) {
    case "j":
    case "ArrowDown":
      if (isExpanded) {
        const keys = Object.keys(entry.data);
        if (detailCursor < keys.length) {
          detailCursor += 1;
          renderRows();
          ev.preventDefault();
          return;
        }
      }
      setCursor(cursor + 1);
      ev.preventDefault();
      break;
    case "k":
    case "ArrowUp":
      if (isExpanded && detailCursor > 0) {
        detailCursor -= 1;
        renderRows();
        ev.preventDefault();
        return;
      }
      setCursor(cursor - 1);
      ev.preventDefault();
      break;
    case "u":
      if (isExpanded) {
        toggleExpand(entry.id);
        ev.preventDefault();
        return;
      }
      setCursor(cursor - 1);
      ev.preventDefault();
      break;
    case "o":
    case "Enter":
      if (entry) {
        if (follow) follow = false;
        toggleExpand(entry.id);
      }
      ev.preventDefault();
      break;
    case "Escape":
      if (isExpanded) {
        toggleExpand(entry.id);
        ev.preventDefault();
      }
      break;
    case "g":
      setCursor(0);
      ev.preventDefault();
      break;
    case "G":
      setCursor(entries.length - 1);
      ev.preventDefault();
      break;
    case "f":
      toggleFollow();
      ev.preventDefault();
      break;
    case "v":
      openFields();
      ev.preventDefault();
      break;
    case "t": {
      if (!isExpanded || !entry) return;
      const keys = Object.keys(entry.data);
      if (detailCursor === 0) return;
      const key = keys[detailCursor - 1];
      if (!key) return;
      toggleByKey(key);
      renderHeader();
      renderRows();
      if (!fieldsPopover.hidden) renderFieldsList();
      ev.preventDefault();
      break;
    }
    case "c": {
      if (!entry) return;
      if (isExpanded && detailCursor > 0) {
        const keys = Object.keys(entry.data);
        const key = keys[detailCursor - 1];
        if (key) copyText(stringifyValue(entry.data[key]), key);
      } else {
        copyText(JSON.stringify(entry.data, null, 2), `entry #${entry.id}`);
      }
      ev.preventDefault();
      break;
    }
  }
});

start();
