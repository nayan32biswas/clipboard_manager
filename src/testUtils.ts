// Test helpers: an in-memory Api that records what the views ask for.
import type { Api, ItemView, Settings } from "./api";

export const defaultSettings: Settings = {
  retentionDays: 3,
  captureImages: true,
  skipSecrets: false,
  ignoredApps: [],
  hotkey: "Ctrl+Alt+V",
  launchAtLogin: true,
  primarySelection: true,
};

export function textItem(id: number, preview: string, extra: Partial<ItemView> = {}): ItemView {
  return { id, kind: "text", preview, thumb: null, pinned: false, lastUsedAt: 0, sourceApp: null, ...extra };
}

export class FakeApi implements Api {
  items: ItemView[] = [];
  settings: Settings = { ...defaultSettings };
  calls: string[] = [];
  private listeners: Record<string, (() => void)[]> = {};

  async search(query: string) {
    this.calls.push(`search:${query}`);
    const q = query.toLowerCase();
    return this.items.filter((i) => !q || (i.preview ?? "").toLowerCase().includes(q));
  }
  async copyItem(id: number) {
    this.calls.push(`copy:${id}`);
  }
  async togglePin(id: number) {
    this.calls.push(`pin:${id}`);
    const item = this.items.find((i) => i.id === id)!;
    item.pinned = !item.pinned;
    return item.pinned;
  }
  async deleteItem(id: number) {
    this.calls.push(`delete:${id}`);
    this.items = this.items.filter((i) => i.id !== id);
  }
  async clearHistory() {
    this.calls.push("clear");
    const before = this.items.length;
    this.items = this.items.filter((i) => i.pinned);
    return before - this.items.length;
  }
  async getSettings() {
    return { ...this.settings };
  }
  async setSettings(s: Settings) {
    this.calls.push("setSettings");
    this.settings = { ...s, retentionDays: Math.min(365, Math.max(1, s.retentionDays)) };
    return { ...this.settings };
  }
  async hideWindow() {
    this.calls.push("hide");
  }
  async pauseHotkey() {
    this.calls.push("pauseHotkey");
  }
  async resumeHotkey() {
    this.calls.push("resumeHotkey");
  }
  onHistoryChanged(cb: () => void) {
    return this.on("history-changed", cb);
  }
  onShown(cb: () => void) {
    return this.on("window-shown", cb);
  }
  onOpenSettings(cb: () => void) {
    return this.on("open-settings", cb);
  }
  emit(event: string) {
    (this.listeners[event] ?? []).forEach((cb) => cb());
  }
  private async on(event: string, cb: () => void) {
    (this.listeners[event] ??= []).push(cb);
    return () => {
      this.listeners[event] = this.listeners[event].filter((c) => c !== cb);
    };
  }
}

/** Lets pending promises and zero-delay timers run. */
export const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

export function key(target: EventTarget, k: string, opts: KeyboardEventInit = {}) {
  target.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...opts }));
}
