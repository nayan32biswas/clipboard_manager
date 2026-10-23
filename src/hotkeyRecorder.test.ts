// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { attachHotkeyRecorder, shortcutFromEvent } from "./hotkeyRecorder";
import { FakeApi, flush } from "./testUtils";

const press = (code: string, mods: KeyboardEventInit = {}) =>
  new KeyboardEvent("keydown", { code, key: code, bubbles: true, cancelable: true, ...mods });

describe("shortcutFromEvent", () => {
  it("builds the name the backend parses, modifiers in a fixed order", () => {
    expect(shortcutFromEvent(press("KeyV", { altKey: true, ctrlKey: true }))).toBe("Ctrl+Alt+V");
    expect(shortcutFromEvent(press("Digit5", { shiftKey: true, metaKey: true }))).toBe("Shift+Super+5");
    expect(shortcutFromEvent(press("F9", { ctrlKey: true }))).toBe("Ctrl+F9");
    expect(shortcutFromEvent(press("Space", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+Space");
    expect(shortcutFromEvent(press("ArrowUp", { altKey: true }))).toBe("Alt+Up");
  });

  it("falls back to the key name when there is no code (remote desktop, AutoHotkey)", () => {
    const noCode = (key: string, mods: KeyboardEventInit) => press("", { key, ...mods });
    expect(shortcutFromEvent(noCode("K", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl+Shift+K");
    expect(shortcutFromEvent(noCode("7", { altKey: true }))).toBe("Alt+7");
    expect(shortcutFromEvent(noCode(" ", { ctrlKey: true }))).toBe("Ctrl+Space");
    expect(shortcutFromEvent(noCode("Control", { ctrlKey: true }))).toBeNull();
  });

  it("allows a bare function key", () => {
    expect(shortcutFromEvent(press("F8"))).toBe("F8");
  });

  it("rejects a plain key, a lone modifier and unknown keys", () => {
    expect(shortcutFromEvent(press("KeyV"))).toBeNull();
    expect(shortcutFromEvent(press("ControlLeft", { ctrlKey: true }))).toBeNull();
    expect(shortcutFromEvent(press("IntlRo", { ctrlKey: true }))).toBeNull();
  });
});

describe("hotkey recorder", () => {
  let input: HTMLInputElement;
  let api: FakeApi;
  let message: string;

  beforeEach(() => {
    input = document.createElement("input");
    input.value = "Ctrl+Alt+V";
    document.body.append(input);
    api = new FakeApi();
    message = "";
    attachHotkeyRecorder(input, api, (text) => (message = text));
  });

  afterEach(() => input.remove());

  it("can't be typed into", () => {
    expect(input.readOnly).toBe(true);
  });

  it("pauses the global shortcut while focused so pressing it reaches the page", async () => {
    input.focus();
    await flush();
    expect(api.calls).toEqual(["pauseHotkey"]);
    input.blur();
    await flush();
    expect(api.calls).toEqual(["pauseHotkey", "resumeHotkey"]);
  });

  it("pressing a combination fills it in", () => {
    input.focus();
    const e = press("KeyB", { ctrlKey: true, shiftKey: true });
    input.dispatchEvent(e);
    expect(input.value).toBe("Ctrl+Shift+B");
    expect(e.defaultPrevented).toBe(true);
    expect(message).toMatch(/save/i);
  });

  it("shows held modifiers, then goes back if no key follows", () => {
    input.focus();
    input.dispatchEvent(press("ControlLeft", { ctrlKey: true }));
    input.dispatchEvent(press("AltLeft", { ctrlKey: true, altKey: true }));
    expect(input.value).toBe("Ctrl+Alt+");
    input.dispatchEvent(new KeyboardEvent("keyup", { code: "AltLeft", ctrlKey: true, bubbles: true }));
    expect(input.value).toBe("Ctrl+Alt+");
    input.dispatchEvent(new KeyboardEvent("keyup", { code: "ControlLeft", bubbles: true }));
    expect(input.value).toBe("Ctrl+Alt+V");
  });

  it("explains why a plain key is not accepted and keeps the old value", () => {
    input.focus();
    input.dispatchEvent(press("KeyB"));
    expect(input.value).toBe("Ctrl+Alt+V");
    expect(message).toMatch(/ctrl, alt, shift or super/i);
  });

  it("shows held modifiers even when the event has no code", () => {
    input.focus();
    input.dispatchEvent(press("", { key: "Control", ctrlKey: true }));
    expect(input.value).toBe("Ctrl+");
  });

  it("lets Tab, Enter and Escape do their normal job", () => {
    input.focus();
    for (const code of ["Tab", "Enter", "Escape"]) {
      const e = press(code);
      input.dispatchEvent(e);
      expect(e.defaultPrevented).toBe(false);
    }
    expect(input.value).toBe("Ctrl+Alt+V");
  });

  it("an unfinished combination is dropped when focus leaves", () => {
    input.focus();
    input.dispatchEvent(press("ShiftLeft", { shiftKey: true }));
    input.blur();
    expect(input.value).toBe("Ctrl+Alt+V");
  });
});
