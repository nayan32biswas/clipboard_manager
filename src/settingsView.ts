// The settings page. Reads and writes settings through the Api only.
import type { Api, Settings } from "./api";
import { attachHotkeyRecorder } from "./hotkeyRecorder";

export interface SettingsView {
  load(): Promise<void>;
}

export function parseIgnoredApps(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

export function createSettingsView(
  root: HTMLElement,
  api: Api,
  opts: { onClose: () => void; linux?: boolean },
): SettingsView {
  // Only Linux has the primary selection (middle-click paste).
  const linux = opts.linux ?? navigator.userAgent.includes("Linux");
  root.replaceChildren();
  const form = document.createElement("form");
  form.className = "settings";
  // The backend clamps values and we show what it stored, so skip browser validation.
  form.noValidate = true;
  form.innerHTML = `
    <header><button type="button" class="back" aria-label="Back" title="Back (Esc)">&larr;</button><h1>Settings</h1></header>
    <label>Keep history for <input name="retentionDays" type="number" min="1" max="365" required> days</label>
    <p class="hint">Pinned items are never removed.</p>
    <label><input name="captureImages" type="checkbox"> Save images</label>
    <label><input name="skipSecrets" type="checkbox"> Skip items marked secret by password managers</label>
    <label><input name="launchAtLogin" type="checkbox"> Start when I log in</label>
    <label class="primary-selection"><input name="primarySelection" type="checkbox"> Also paste with middle-click and Shift+Insert</label>
    <label>Shortcut to open <input name="hotkey" type="text" spellcheck="false"></label>
    <p class="hint">Click the box, then press the keys.</p>
    <label>Ignore copies from these apps (one per line)
      <textarea name="ignoredApps" rows="3" spellcheck="false"></textarea></label>
    <div class="actions">
      <button type="submit" class="save">Save</button>
      <button type="button" class="clear danger">Clear history</button>
    </div>
    <p class="status" role="status"></p>`;
  root.append(form);

  const input = (name: string) => form.querySelector<HTMLInputElement>(`[name="${name}"]`)!;
  const ignored = form.querySelector<HTMLTextAreaElement>('[name="ignoredApps"]')!;
  const status = form.querySelector<HTMLElement>(".status")!;
  const clear = form.querySelector<HTMLButtonElement>(".clear")!;
  let confirmClear = false;
  form.querySelector<HTMLElement>(".primary-selection")!.hidden = !linux;
  attachHotkeyRecorder(input("hotkey"), api, (text) => (status.textContent = text));

  function fill(s: Settings) {
    input("retentionDays").value = String(s.retentionDays);
    input("captureImages").checked = s.captureImages;
    input("skipSecrets").checked = s.skipSecrets;
    input("launchAtLogin").checked = s.launchAtLogin;
    input("primarySelection").checked = s.primarySelection;
    input("hotkey").value = s.hotkey;
    ignored.value = s.ignoredApps.join("\n");
  }

  function read(): Settings {
    return {
      retentionDays: Number(input("retentionDays").value) || 1,
      captureImages: input("captureImages").checked,
      skipSecrets: input("skipSecrets").checked,
      launchAtLogin: input("launchAtLogin").checked,
      primarySelection: input("primarySelection").checked,
      hotkey: input("hotkey").value.trim(),
      ignoredApps: parseIgnoredApps(ignored.value),
    };
  }

  async function load() {
    status.textContent = "";
    confirmClear = false;
    clear.textContent = "Clear history";
    fill(await api.getSettings());
  }

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    try {
      fill(await api.setSettings(read()));
      status.textContent = "Saved.";
    } catch (err) {
      status.textContent = `Could not save: ${err}`;
    }
  });

  clear.addEventListener("click", async () => {
    if (!confirmClear) {
      confirmClear = true;
      clear.textContent = "Click again to clear";
      return;
    }
    confirmClear = false;
    clear.textContent = "Clear history";
    const removed = await api.clearHistory();
    status.textContent = `Removed ${removed} item${removed === 1 ? "" : "s"}. Pinned items were kept.`;
  });

  form.querySelector(".back")!.addEventListener("click", () => opts.onClose());
  form.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      opts.onClose();
    }
  });

  return { load };
}
