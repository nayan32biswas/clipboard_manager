// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mountApp } from "./main";
import { FakeApi, flush, key, textItem } from "./testUtils";

describe("app shell", () => {
  let root: HTMLElement;
  let api: FakeApi;

  beforeEach(async () => {
    root = document.createElement("main");
    document.body.append(root);
    api = new FakeApi();
    api.items = [textItem(1, "first"), textItem(2, "second")];
    mountApp(root, api);
    await flush();
  });

  afterEach(() => root.remove());

  const layer = () => root.querySelector<HTMLElement>(".settings-layer")!;
  const list = () => root.querySelector<HTMLElement>(".history")!;
  const search = () => root.querySelector<HTMLInputElement>("input.search")!;

  it("starts on the list with the settings layer closed", () => {
    expect(layer().hidden).toBe(true);
    expect(list().hidden).toBe(false);
    expect(root.querySelectorAll(".item")).toHaveLength(2);
  });

  it("the gear opens settings as a layer on top of the list", async () => {
    root.querySelector<HTMLButtonElement>("button.open-settings")!.click();
    await flush();
    expect(layer().hidden).toBe(false);
    expect(layer().getAttribute("role")).toBe("dialog");
    // The list stays underneath, untouched, so closing returns to the same place.
    expect(list().hidden).toBe(false);
    expect(root.querySelectorAll(".item")).toHaveLength(2);
    expect(layer().querySelector<HTMLInputElement>('[name="retentionDays"]')!.value).toBe("3");
    expect(document.activeElement).toBe(layer().querySelector("input"));
  });

  it("Back closes the layer and puts focus back in search", async () => {
    root.querySelector<HTMLButtonElement>("button.open-settings")!.click();
    await flush();
    layer().querySelector<HTMLButtonElement>("button.back")!.click();
    expect(layer().hidden).toBe(true);
    expect(document.activeElement).toBe(search());
  });

  it("Escape in settings closes only the layer, not the window", async () => {
    root.querySelector<HTMLButtonElement>("button.open-settings")!.click();
    await flush();
    key(layer().querySelector("input")!, "Escape");
    await flush();
    expect(layer().hidden).toBe(true);
    expect(api.calls).not.toContain("hide");
  });

  it("list shortcuts do not fire while settings are open", async () => {
    root.querySelector<HTMLButtonElement>("button.open-settings")!.click();
    await flush();
    key(layer().querySelector("input")!, "Enter");
    key(layer().querySelector("input")!, "Delete");
    await flush();
    expect(api.calls.filter((c) => c.startsWith("copy:") || c.startsWith("delete:"))).toEqual([]);
  });

  it("the tray's Settings item opens the layer", async () => {
    api.emit("open-settings");
    await flush();
    expect(layer().hidden).toBe(false);
  });

  it("showing the window again closes the layer", async () => {
    api.emit("open-settings");
    await flush();
    api.emit("window-shown");
    await flush();
    expect(layer().hidden).toBe(true);
  });
});
