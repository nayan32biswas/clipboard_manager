// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSettingsView, parseIgnoredApps } from "./settingsView";
import { FakeApi, flush, key } from "./testUtils";

describe("parseIgnoredApps", () => {
  it("keeps one trimmed name per non-empty line", () => {
    expect(parseIgnoredApps(" KeePassXC \n\n1Password\r\n  ")).toEqual(["KeePassXC", "1Password"]);
  });
});

describe("settings view", () => {
  let root: HTMLElement;
  let api: FakeApi;
  let onClose: ReturnType<typeof vi.fn<() => void>>;

  beforeEach(() => {
    root = document.createElement("div");
    document.body.append(root);
    api = new FakeApi();
    api.settings = { ...api.settings, ignoredApps: ["KeePassXC"], retentionDays: 7 };
    onClose = vi.fn<() => void>();
  });

  afterEach(() => root.remove());

  const field = <T extends HTMLElement>(name: string) => root.querySelector<T>(`[name="${name}"]`)!;
  const button = (label: RegExp) => [...root.querySelectorAll("button")].find((b) => label.test(b.textContent ?? ""))!;

  async function mount(linux = true) {
    const view = createSettingsView(root, api, { onClose, linux });
    await view.load();
    return view;
  }

  it("fills the form from the backend", async () => {
    await mount();
    expect(field<HTMLInputElement>("retentionDays").value).toBe("7");
    expect(field<HTMLInputElement>("captureImages").checked).toBe(true);
    expect(field<HTMLInputElement>("skipSecrets").checked).toBe(false);
    expect(field<HTMLInputElement>("launchAtLogin").checked).toBe(true);
    expect(field<HTMLInputElement>("hotkey").value).toBe("Ctrl+Alt+V");
    expect(field<HTMLTextAreaElement>("ignoredApps").value).toBe("KeePassXC");
  });

  it("saves edited values and shows what the backend stored", async () => {
    await mount();
    field<HTMLInputElement>("retentionDays").value = "999";
    field<HTMLInputElement>("skipSecrets").checked = true;
    field<HTMLTextAreaElement>("ignoredApps").value = "KeePassXC\nBitwarden";
    button(/save/i).click();
    await flush();

    expect(api.settings).toMatchObject({ retentionDays: 365, skipSecrets: true, ignoredApps: ["KeePassXC", "Bitwarden"] });
    expect(field<HTMLInputElement>("retentionDays").value).toBe("365");
    expect(root.querySelector(".status")?.textContent).toMatch(/saved/i);
  });

  it("the middle-click option is on by default and can be turned off", async () => {
    await mount();
    const primary = field<HTMLInputElement>("primarySelection");
    expect(primary.checked).toBe(true);
    primary.checked = false;
    button(/save/i).click();
    await flush();
    expect(api.settings.primarySelection).toBe(false);
  });

  it("hides the middle-click option off Linux", async () => {
    await mount(false);
    expect(field("primarySelection").closest("label")!.hidden).toBe(true);
  });

  it("shows the error when saving fails", async () => {
    await mount();
    api.setSettings = async () => {
      throw "hotkey is not valid";
    };
    button(/save/i).click();
    await flush();
    expect(root.querySelector(".status")?.textContent).toMatch(/hotkey is not valid/);
  });

  it("clear history needs a second click to confirm", async () => {
    await mount();
    const clear = button(/clear/i);
    clear.click();
    await flush();
    expect(api.calls).not.toContain("clear");
    expect(clear.textContent).toMatch(/again/i);
    clear.click();
    await flush();
    expect(api.calls).toContain("clear");
  });

  it("the shortcut is recorded by pressing it, then saved", async () => {
    await mount();
    const hotkey = field<HTMLInputElement>("hotkey");
    expect(hotkey.readOnly).toBe(true);
    hotkey.focus();
    hotkey.dispatchEvent(
      new KeyboardEvent("keydown", { code: "KeyB", key: "b", ctrlKey: true, altKey: true, bubbles: true, cancelable: true }),
    );
    expect(root.querySelector(".status")?.textContent).toMatch(/save/i);
    button(/save/i).click();
    await flush();
    expect(api.settings.hotkey).toBe("Ctrl+Alt+B");
  });

  it("Back and Escape close the page", async () => {
    await mount();
    root.querySelector<HTMLButtonElement>("button.back")!.click();
    key(field("hotkey"), "Escape");
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
