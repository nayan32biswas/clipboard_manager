import { tauriApi, type Api } from "./api";
import { createHistoryView } from "./historyView";
import { createSettingsView } from "./settingsView";

/** The list, with settings as a layer on top of it. */
export function mountApp(root: HTMLElement, api: Api) {
  const historyRoot = document.createElement("section");
  historyRoot.className = "history";
  // A sibling of the list, so list shortcuts (Enter, Delete) don't fire in it.
  const layer = document.createElement("section");
  layer.className = "settings-layer";
  layer.setAttribute("role", "dialog");
  layer.setAttribute("aria-label", "Settings");
  layer.hidden = true;
  root.append(historyRoot, layer);

  const history = createHistoryView(historyRoot, api, { onOpenSettings: openSettings });
  const settings = createSettingsView(layer, api, { onClose: closeSettings });

  function openSettings() {
    layer.hidden = false;
    layer.scrollTop = 0;
    void settings.load().then(() => layer.querySelector<HTMLElement>("input")?.focus());
  }

  function closeSettings() {
    layer.hidden = true;
    history.focusSearch();
  }

  void api.onShown(() => {
    if (!layer.hidden) closeSettings();
  });
  void api.onOpenSettings(openSettings);

  void history.refresh();
  history.focusSearch();
}

const appRoot = document.querySelector<HTMLElement>("#app");
if (appRoot) mountApp(appRoot, tauriApi);
