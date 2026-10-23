// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createHistoryView, moveSelection, relativeTime } from "./historyView";
import { FakeApi, flush, key, textItem } from "./testUtils";

const NOW = 1_000_000_000;

describe("relativeTime", () => {
  it.each([
    [NOW - 5_000, "just now"],
    [NOW - 3 * 60_000, "3m ago"],
    [NOW - 2 * 3_600_000, "2h ago"],
    [NOW - 3 * 86_400_000, "3d ago"],
    [NOW + 10_000, "just now"],
  ])("formats %d", (ms, expected) => {
    expect(relativeTime(ms, NOW)).toBe(expected);
  });
});

describe("moveSelection", () => {
  it("stays inside the list", () => {
    expect(moveSelection(0, -1, 3)).toBe(0);
    expect(moveSelection(2, 1, 3)).toBe(2);
    expect(moveSelection(1, 1, 3)).toBe(2);
    expect(moveSelection(0, 1, 0)).toBe(0);
  });
});

describe("history view", () => {
  let root: HTMLElement;
  let api: FakeApi;
  let openSettings: ReturnType<typeof vi.fn<() => void>>;

  beforeEach(() => {
    root = document.createElement("div");
    document.body.append(root);
    api = new FakeApi();
    api.items = [textItem(3, "newest", { lastUsedAt: NOW - 60_000 }), textItem(2, "middle"), textItem(1, "oldest")];
    openSettings = vi.fn<() => void>();
  });

  afterEach(() => {
    root.remove();
    vi.useRealTimers();
  });

  async function mount() {
    const view = createHistoryView(root, api, { onOpenSettings: openSettings, now: () => NOW });
    await view.refresh();
    return view;
  }

  const rows = () => [...root.querySelectorAll<HTMLElement>(".item")];
  const rowTexts = () => rows().map((r) => r.querySelector(".preview")?.textContent);
  const selectedText = () => root.querySelector(".item.selected .preview")?.textContent;
  const search = () => root.querySelector<HTMLInputElement>("input.search")!;

  it("lists items in the order the backend returns, first one selected", async () => {
    await mount();
    expect(rowTexts()).toEqual(["newest", "middle", "oldest"]);
    expect(selectedText()).toBe("newest");
    expect(rows()[0].querySelector(".time")?.textContent).toBe("1m ago");
  });

  it("shows a hint when there is nothing", async () => {
    api.items = [];
    await mount();
    expect(rows()).toHaveLength(0);
    expect(root.querySelector(".empty")?.textContent).toMatch(/copy something/i);
  });

  it("marks pinned rows and shows image thumbnails", async () => {
    api.items = [
      textItem(5, "pinned one", { pinned: true }),
      { ...textItem(4, ""), kind: "image", preview: null, thumb: "data:image/png;base64,AAAA" },
    ];
    await mount();
    expect(rows()[0].classList.contains("pinned")).toBe(true);
    expect(rows()[1].querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AAAA");
  });

  it("does not render item text as HTML", async () => {
    api.items = [textItem(1, "<b>bold</b>")];
    await mount();
    expect(root.querySelector(".preview b")).toBeNull();
    expect(rowTexts()).toEqual(["<b>bold</b>"]);
  });

  it("searches 100 ms after typing stops", async () => {
    await mount();
    vi.useFakeTimers();
    api.calls = [];
    search().value = "mid";
    search().dispatchEvent(new Event("input"));
    search().value = "midd";
    search().dispatchEvent(new Event("input"));
    await vi.advanceTimersByTimeAsync(99);
    expect(api.calls).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(api.calls).toEqual(["search:midd"]);
    expect(rowTexts()).toEqual(["middle"]);
  });

  it("arrow keys move the selection and Enter copies it", async () => {
    await mount();
    key(search(), "ArrowDown");
    key(search(), "ArrowDown");
    key(search(), "ArrowDown");
    expect(selectedText()).toBe("oldest");
    key(search(), "ArrowUp");
    expect(selectedText()).toBe("middle");
    key(search(), "Enter");
    await flush();
    expect(api.calls).toContain("copy:2");
  });

  it("Enter right after typing copies from the new results, not the old list", async () => {
    await mount();
    vi.useFakeTimers();
    api.calls = [];
    search().value = "old";
    search().dispatchEvent(new Event("input"));
    key(search(), "Enter");
    await vi.advanceTimersByTimeAsync(0);
    expect(api.calls).toEqual(["search:old", "copy:1"]);
    // The pending search already ran, so it doesn't run again.
    await vi.advanceTimersByTimeAsync(200);
    expect(api.calls).toEqual(["search:old", "copy:1"]);
  });

  it("clicking a row while a search is pending copies that row", async () => {
    await mount();
    vi.useFakeTimers();
    search().value = "old";
    search().dispatchEvent(new Event("input"));
    rows()[1].click();
    await vi.advanceTimersByTimeAsync(0);
    expect(api.calls).toContain("copy:2");
  });

  it("clicking a row copies it", async () => {
    await mount();
    rows()[2].click();
    await flush();
    expect(api.calls).toContain("copy:1");
  });

  it("Ctrl+P toggles pin on the selected item and refreshes", async () => {
    await mount();
    key(search(), "ArrowDown");
    key(search(), "p", { ctrlKey: true });
    await flush();
    expect(api.calls).toContain("pin:2");
    expect(rows()[1].classList.contains("pinned")).toBe(true);
  });

  it("Delete removes the selected item and keeps a nearby selection", async () => {
    await mount();
    key(search(), "ArrowDown");
    key(search(), "Delete");
    await flush();
    expect(api.calls).toContain("delete:2");
    expect(rowTexts()).toEqual(["newest", "oldest"]);
    expect(selectedText()).toBe("oldest");
  });

  it("Delete edits the search text instead when the caret is not at the end", async () => {
    await mount();
    search().value = "abc";
    search().setSelectionRange(0, 0);
    key(search(), "Delete");
    await flush();
    expect(api.calls.some((c) => c.startsWith("delete:"))).toBe(false);
  });

  it("the gear button next to search opens settings", async () => {
    await mount();
    const gear = root.querySelector<HTMLButtonElement>("button.open-settings")!;
    expect(gear.getAttribute("aria-label")).toBe("Settings");
    expect(gear.previousElementSibling).toBe(search());
    gear.click();
    expect(openSettings).toHaveBeenCalledTimes(1);
  });

  it("Escape hides the window and Ctrl+, opens settings", async () => {
    await mount();
    key(search(), "Escape");
    key(search(), ",", { ctrlKey: true });
    await flush();
    expect(api.calls).toContain("hide");
    expect(openSettings).toHaveBeenCalled();
  });

  it("refreshes when the backend reports a new clip", async () => {
    await mount();
    api.items.unshift(textItem(9, "fresh copy"));
    api.emit("history-changed");
    await flush();
    expect(rowTexts()[0]).toBe("fresh copy");
  });

  it("when shown again, clears the search, selects the top row and focuses search", async () => {
    await mount();
    search().value = "old";
    search().dispatchEvent(new Event("input"));
    await new Promise((r) => setTimeout(r, 120));
    key(search(), "ArrowDown");
    api.emit("window-shown");
    await flush();
    expect(search().value).toBe("");
    expect(rowTexts()).toEqual(["newest", "middle", "oldest"]);
    expect(selectedText()).toBe("newest");
    expect(document.activeElement).toBe(search());
  });
});
