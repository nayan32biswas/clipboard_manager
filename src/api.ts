// The only file that talks to the Rust backend. Views get an `Api` passed
// in, so tests can use a fake.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ItemView {
  id: number;
  kind: "text" | "image";
  preview: string | null;
  thumb: string | null;
  pinned: boolean;
  lastUsedAt: number;
  sourceApp: string | null;
}

export interface Settings {
  retentionDays: number;
  captureImages: boolean;
  skipSecrets: boolean;
  ignoredApps: string[];
  hotkey: string;
  launchAtLogin: boolean;
  /** Linux only: also paste picked text with middle-click and Shift+Insert. */
  primarySelection: boolean;
}

export type Unlisten = () => void;

export interface Api {
  search(query: string): Promise<ItemView[]>;
  copyItem(id: number): Promise<void>;
  togglePin(id: number): Promise<boolean>;
  deleteItem(id: number): Promise<void>;
  clearHistory(): Promise<number>;
  getSettings(): Promise<Settings>;
  setSettings(settings: Settings): Promise<Settings>;
  hideWindow(): Promise<void>;
  /** Turn the global shortcut off while a new one is being recorded. */
  pauseHotkey(): Promise<void>;
  resumeHotkey(): Promise<void>;
  /** A clip was added or moved to the top. */
  onHistoryChanged(cb: () => void): Promise<Unlisten>;
  /** The window was just shown (hotkey, tray or second launch). */
  onShown(cb: () => void): Promise<Unlisten>;
  /** The tray asked for the settings page. */
  onOpenSettings(cb: () => void): Promise<Unlisten>;
}

export const tauriApi: Api = {
  search: (query) => invoke("search", { query }),
  copyItem: (id) => invoke("copy_item", { id }),
  togglePin: (id) => invoke("toggle_pin", { id }),
  deleteItem: (id) => invoke("delete_item", { id }),
  clearHistory: () => invoke("clear_history"),
  getSettings: () => invoke("get_settings"),
  setSettings: (settings) => invoke("set_settings", { settings }),
  hideWindow: () => invoke("hide_window"),
  pauseHotkey: () => invoke("pause_hotkey"),
  resumeHotkey: () => invoke("resume_hotkey"),
  onHistoryChanged: (cb) => listen("history-changed", () => cb()),
  onShown: (cb) => listen("window-shown", () => cb()),
  onOpenSettings: (cb) => listen("open-settings", () => cb()),
};
