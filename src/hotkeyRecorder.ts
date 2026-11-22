// The shortcut field: you press the keys instead of typing their names.
import type { Api } from "./api";

const MODIFIER_CODE = /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/;
// Keys that keep their normal job (move focus, save, close settings).
const PASS_THROUGH = new Set(["Tab", "Enter", "Escape"]);
// e.code names the backend parser also accepts, minus the arrow prefix.
const NAMED_KEYS = new Set([
  "Space", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown",
  "Minus", "Equal", "BracketLeft", "BracketRight", "Backslash", "Semicolon", "Quote",
  "Comma", "Period", "Slash", "Backquote",
]);

function modifiers(e: KeyboardEvent): string[] {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  return mods;
}

/** Prefers e.code, so the name doesn't depend on keyboard layout or Shift.
 * Injected keys (remote desktop, AutoHotkey) can arrive with no code. */
function codeOf(e: KeyboardEvent): string {
  if (e.code) return e.code;
  if (/^[a-z]$/i.test(e.key)) return `Key${e.key.toUpperCase()}`;
  if (/^\d$/.test(e.key)) return `Digit${e.key}`;
  return e.key === " " ? "Space" : e.key;
}

function keyName(code: string): string | null {
  const m = /^(?:Key([A-Z])|Digit(\d))$/.exec(code);
  if (m) return m[1] ?? m[2];
  if (/^F\d{1,2}$/.test(code) || /^Numpad\w+$/.test(code)) return code;
  if (code.startsWith("Arrow")) return code.slice("Arrow".length);
  return NAMED_KEYS.has(code) ? code : null;
}

/** "Ctrl+Alt+V" for a usable combination, otherwise null. */
export function shortcutFromEvent(e: KeyboardEvent): string | null {
  const key = keyName(codeOf(e));
  if (!key) return null;
  const mods = modifiers(e);
  // A bare letter would fire on every keystroke in every app.
  if (mods.length === 0 && !/^F\d/.test(key)) return null;
  return [...mods, key].join("+");
}

export function attachHotkeyRecorder(input: HTMLInputElement, api: Api, onMessage: (text: string) => void) {
  input.readOnly = true;
  input.placeholder = "Press a shortcut";
  let last = input.value;

  const isPreview = () => input.value.endsWith("+");

  // Otherwise pressing the current shortcut toggles the window instead of
  // reaching this field. The backend also turns it back on when the window hides.
  input.addEventListener("focus", () => {
    last = input.value;
    input.classList.add("recording");
    onMessage("Press the new shortcut.");
    void api.pauseHotkey();
  });

  input.addEventListener("blur", () => {
    if (isPreview()) input.value = last;
    input.classList.remove("recording");
    void api.resumeHotkey();
  });

  input.addEventListener("keydown", (e) => {
    if (PASS_THROUGH.has(e.key) && modifiers(e).length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    if (MODIFIER_CODE.test(codeOf(e))) {
      input.value = modifiers(e).join("+") + "+";
      return;
    }
    const shortcut = shortcutFromEvent(e);
    if (shortcut) {
      input.value = last = shortcut;
      onMessage(`${shortcut} — press Save to use it.`);
    } else {
      input.value = last;
      onMessage(keyName(codeOf(e)) ? "Add Ctrl, Alt, Shift or Super to the key." : "That key can't be used.");
    }
  });

  input.addEventListener("keyup", (e) => {
    if (isPreview() && modifiers(e).length === 0) input.value = last;
  });
}
