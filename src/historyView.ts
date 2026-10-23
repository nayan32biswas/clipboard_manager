// The history list: search box, rows, keyboard. Draws what the Api returns
// and sends user actions back. No storage logic lives here.
import type { Api, ItemView } from "./api";

export interface HistoryView {
  refresh(): Promise<void>;
  focusSearch(): void;
}

export interface HistoryOptions {
  onOpenSettings: () => void;
  now?: () => number;
}

const SEARCH_DEBOUNCE_MS = 100;

const GEAR_ICON = `<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none"
  stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="12" r="3"/>
  <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33
    1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06
    a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09
    A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68
    a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06
    a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09
    a1.65 1.65 0 0 0-1.51 1z"/>
</svg>`;

export function relativeTime(ms: number, now: number): string {
  const seconds = Math.floor((now - ms) / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export function moveSelection(index: number, delta: number, length: number): number {
  if (length === 0) return 0;
  return Math.min(length - 1, Math.max(0, index + delta));
}

export function createHistoryView(root: HTMLElement, api: Api, opts: HistoryOptions): HistoryView {
  const now = opts.now ?? Date.now;
  let items: ItemView[] = [];
  let selected = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  root.replaceChildren();
  const search = document.createElement("input");
  search.className = "search";
  search.type = "search";
  search.placeholder = "Search history";
  search.spellcheck = false;
  const gear = document.createElement("button");
  gear.type = "button";
  gear.className = "open-settings";
  gear.title = "Settings (Ctrl+,)";
  gear.setAttribute("aria-label", "Settings");
  gear.innerHTML = GEAR_ICON;
  gear.addEventListener("click", () => opts.onOpenSettings());
  const toolbar = document.createElement("div");
  toolbar.className = "toolbar";
  toolbar.append(search, gear);
  const list = document.createElement("ul");
  list.className = "list";
  const empty = document.createElement("p");
  empty.className = "empty";
  root.append(toolbar, list, empty);

  async function refresh() {
    items = await api.search(search.value);
    selected = moveSelection(selected, 0, items.length);
    render();
  }

  function render() {
    list.replaceChildren(...items.map(renderRow));
    empty.textContent = items.length
      ? ""
      : search.value
        ? "No matches."
        : "No items yet. Copy something and it shows up here.";
    list.querySelector(".selected")?.scrollIntoView?.({ block: "nearest" });
  }

  function renderRow(item: ItemView, index: number): HTMLLIElement {
    const li = document.createElement("li");
    li.className = "item";
    li.classList.toggle("selected", index === selected);
    li.classList.toggle("pinned", item.pinned);
    li.dataset.id = String(item.id);

    if (item.kind === "image" && item.thumb) {
      const img = document.createElement("img");
      img.className = "thumb";
      img.src = item.thumb;
      img.alt = "Image";
      li.append(img);
    } else {
      const preview = document.createElement("div");
      preview.className = "preview";
      preview.textContent = item.preview ?? "";
      li.append(preview);
    }

    const meta = document.createElement("div");
    meta.className = "meta";
    const time = document.createElement("span");
    time.className = "time";
    time.textContent = relativeTime(item.lastUsedAt, now());
    meta.append(time);
    if (item.pinned) {
      const pin = document.createElement("span");
      pin.className = "pin";
      pin.textContent = "Pinned";
      meta.append(pin);
    }
    li.append(meta);

    li.addEventListener("click", () => {
      selected = index;
      void copySelected();
    });
    return li;
  }

  const current = () => items[selected];

  async function copySelected() {
    const item = current();
    if (item) await api.copyItem(item.id);
  }

  async function pinSelected() {
    const item = current();
    if (!item) return;
    await api.togglePin(item.id);
    await refresh();
  }

  async function deleteSelected() {
    const item = current();
    if (!item) return;
    await api.deleteItem(item.id);
    await refresh();
  }

  function move(delta: number) {
    selected = moveSelection(selected, delta, items.length);
    render();
  }

  async function runSearch() {
    clearTimeout(timer);
    timer = undefined;
    selected = 0;
    await refresh();
  }

  async function flushSearch() {
    if (timer !== undefined) await runSearch();
  }

  search.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => void runSearch(), SEARCH_DEBOUNCE_MS);
  });

  root.addEventListener("keydown", (e) => {
    const ctrl = e.ctrlKey || e.metaKey;
    const caretAtEnd = search.selectionStart === search.value.length;
    let handled = true;
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    // Enter can arrive before the search runs. Copy from the new results.
    else if (e.key === "Enter") void flushSearch().then(copySelected);
    else if (e.key === "Escape") void api.hideWindow();
    else if (ctrl && e.key.toLowerCase() === "p") void pinSelected();
    else if (ctrl && e.key === ",") opts.onOpenSettings();
    else if (e.key === "Delete" && caretAtEnd) void deleteSelected();
    else handled = false;
    if (handled) e.preventDefault();
  });

  function focusSearch() {
    search.focus();
    search.select();
  }

  void api.onHistoryChanged(() => void refresh());
  void api.onShown(() => {
    clearTimeout(timer);
    timer = undefined;
    search.value = "";
    selected = 0;
    focusSearch();
    void refresh();
  });

  return { refresh, focusSearch };
}
