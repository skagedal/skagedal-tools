// Browser-side log viewer. Fetches config + entries via SSE and renders a
// simple table with vi-like navigation (j/k/u/o, g/G, esc).

const headerRow = document.getElementById("header-row");
const rowsEl = document.getElementById("rows");
const statusEl = document.getElementById("status");
const metaEl = document.getElementById("meta");
const detailEl = document.getElementById("detail");
const detailBody = document.getElementById("detail-body");
const detailId = document.getElementById("detail-id");

let config = null;
let entries = [];
let cursor = 0;
let view = "list";
let done = false;

async function start() {
  const meta = await fetch("/api/meta").then((r) => r.json());
  config = meta.config;
  metaEl.textContent = `· ${meta.sourceLabel}`;
  renderHeader();
  startStream();
}

function renderHeader() {
  headerRow.replaceChildren();
  for (const field of config.fields) {
    const th = document.createElement("th");
    th.textContent = field.name;
    headerRow.appendChild(th);
  }
}

function startStream() {
  const es = new EventSource("/api/stream");
  es.addEventListener("entry", (ev) => {
    const entry = JSON.parse(ev.data);
    entries.push(entry);
    appendRow(entry);
    updateStatus();
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

function appendRow(entry) {
  const tr = document.createElement("tr");
  tr.dataset.id = String(entry.id);
  for (const field of config.fields) {
    const td = document.createElement("td");
    const value = pickField(entry.data, field.from);
    td.textContent = value;
    if (field.name.toLowerCase() === "level") {
      td.classList.add(`level-${classifyLevel(value)}`);
    }
    tr.appendChild(td);
  }
  tr.addEventListener("click", () => {
    setCursor(entries.findIndex((e) => e.id === entry.id));
    openDetail();
  });
  rowsEl.appendChild(tr);
  if (entries.length === 1) setCursor(0);
}

function pickField(data, candidates) {
  for (const key of candidates) {
    const value = data[key];
    if (value === undefined || value === null || value === "") continue;
    return typeof value === "string" ? value : JSON.stringify(value);
  }
  return "";
}

function classifyLevel(level) {
  const v = String(level).toLowerCase();
  if (v.includes("error") || v === "err" || v === "fatal") return "error";
  if (v.includes("warn")) return "warn";
  if (v.includes("info")) return "info";
  if (v.includes("debug") || v.includes("trace")) return "debug";
  return "other";
}

function setCursor(i) {
  if (entries.length === 0) return;
  cursor = Math.max(0, Math.min(entries.length - 1, i));
  for (const el of rowsEl.querySelectorAll("tr.selected")) el.classList.remove("selected");
  const sel = rowsEl.children[cursor];
  if (sel) {
    sel.classList.add("selected");
    sel.scrollIntoView({ block: "nearest" });
  }
  updateStatus();
}

function updateStatus() {
  statusEl.textContent = `${entries.length === 0 ? 0 : cursor + 1}/${entries.length}${done ? " (eof)" : ""}`;
}

function openDetail() {
  const entry = entries[cursor];
  if (!entry) return;
  view = "detail";
  detailId.textContent = `#${entry.id}${entry.wrapped ? " (wrapped)" : ""}`;
  detailBody.textContent = JSON.stringify(entry.data, null, 2);
  detailEl.hidden = false;
}

function closeDetail() {
  view = "list";
  detailEl.hidden = true;
}

document.addEventListener("keydown", (ev) => {
  if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
  if (view === "detail") {
    if (ev.key === "Escape" || ev.key === "u" || ev.key === "q") {
      closeDetail();
      ev.preventDefault();
    }
    return;
  }
  switch (ev.key) {
    case "j":
    case "ArrowDown":
      setCursor(cursor + 1);
      ev.preventDefault();
      break;
    case "k":
    case "ArrowUp":
      setCursor(cursor - 1);
      ev.preventDefault();
      break;
    case "u":
      setCursor(cursor - 1);
      ev.preventDefault();
      break;
    case "o":
    case "Enter":
      openDetail();
      ev.preventDefault();
      break;
    case "g":
      setCursor(0);
      ev.preventDefault();
      break;
    case "G":
      setCursor(entries.length - 1);
      ev.preventDefault();
      break;
  }
});

start();
