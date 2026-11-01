"use strict";

const byId = (id) => document.getElementById(id);
const address = byId("address");
const back = byId("back");
const forward = byId("forward");
const reload = byId("reload");
const progress = byId("progress");
const tabsElement = byId("tabs");

const CLOSE_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>';

const FOLDER_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>';

let strip = { tabs: [], activeId: null };
let drag = null;

const send = (command) => window.browserBridge.send(JSON.stringify(command));
const activeTab = () => strip.tabs.find((tab) => tab.id === strip.activeId);

function shownUrl(url) {
  return url === "about:blank" ? "" : url;
}

function tabLabel(tab) {
  const isBlank = !shownUrl(tab.url);
  // A tab's first load keeps the blank page's title until the new title arrives.
  const hasTitle = tab.title && tab.title !== "about:blank";
  return isBlank ? "New tab" : hasTitle ? tab.title : tab.url;
}

function createTabElement(id) {
  const element = document.createElement("div");
  element.className = "tab";
  element.dataset.id = id;
  element.setAttribute("role", "tab");
  element.tabIndex = 0;

  const spinner = document.createElement("span");
  spinner.className = "tab-spinner";
  const title = document.createElement("span");
  title.className = "tab-title";
  const close = document.createElement("button");
  close.className = "icon-button tab-close";
  close.title = "Close tab";
  close.setAttribute("aria-label", "Close tab");
  close.tabIndex = -1;
  close.innerHTML = CLOSE_ICON;
  element.append(spinner, title, close);

  close.addEventListener("pointerdown", (event) => event.stopPropagation());
  close.addEventListener("click", (event) => {
    event.stopPropagation();
    send({ type: "closeTab", id });
  });
  element.addEventListener("auxclick", (event) => {
    if (event.button === 1) {
      send({ type: "closeTab", id });
    }
  });
  element.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      send({ type: "selectTab", id });
    }
  });
  element.addEventListener("pointerdown", (event) => startDrag(event, id, element));
  return element;
}

function startDrag(event, id, element) {
  if (event.button !== 0) {
    return;
  }
  send({ type: "selectTab", id });
  drag = { id, element, startX: event.clientX, moved: false };
  element.setPointerCapture(event.pointerId);
}

function continueDrag(event) {
  if (!drag) {
    return;
  }
  if (!drag.moved && Math.abs(event.clientX - drag.startX) < 6) {
    return;
  }
  drag.moved = true;
  drag.element.classList.add("dragging");

  const others = [...tabsElement.children].filter((child) => child !== drag.element);
  const before = others.find((child) => {
    const box = child.getBoundingClientRect();
    return event.clientX < box.left + box.width / 2;
  });
  tabsElement.insertBefore(drag.element, before || null);
}

function finishDrag() {
  if (!drag) {
    return;
  }
  const { id, element, moved } = drag;
  drag = null;
  element.classList.remove("dragging");
  if (moved) {
    send({ type: "moveTab", id, index: [...tabsElement.children].indexOf(element) });
  }
  render();
}

function renderTabs() {
  const existing = new Map([...tabsElement.children].map((child) => [Number(child.dataset.id), child]));
  for (const tab of strip.tabs) {
    const element = existing.get(tab.id) || createTabElement(tab.id);
    existing.delete(tab.id);
    // Page titles are untrusted, so they're only ever set as text.
    element.querySelector(".tab-title").textContent = tabLabel(tab);
    element.title = tabLabel(tab);
    element.setAttribute("aria-selected", String(tab.id === strip.activeId));
    element.querySelector(".tab-spinner").hidden = !tab.isLoading;
    tabsElement.append(element);
  }
  for (const element of existing.values()) {
    element.remove();
  }
}

function render() {
  if (!drag) {
    renderTabs();
  }

  const tab = activeTab();
  if (!tab) {
    return;
  }
  back.disabled = !tab.canGoBack;
  forward.disabled = !tab.canGoForward;

  const label = tab.isLoading ? "Stop" : "Reload";
  reload.title = label;
  reload.setAttribute("aria-label", label);
  // SVG elements have no `hidden` property, only the attribute.
  byId("reload-icon").toggleAttribute("hidden", tab.isLoading);
  byId("stop-icon").toggleAttribute("hidden", !tab.isLoading);

  byId("lock-icon").toggleAttribute("hidden", !tab.url.startsWith("https://"));
  const star = byId("star");
  star.disabled = !/^https?:\/\//.test(tab.url);
  star.setAttribute("aria-pressed", String(tab.isBookmarked));
  const starLabel = tab.isBookmarked ? "Remove bookmark" : "Bookmark this page";
  star.title = starLabel;
  star.setAttribute("aria-label", starLabel);

  if (document.activeElement !== address) {
    address.value = shownUrl(tab.url);
  }

  progress.style.opacity = tab.isLoading ? "1" : "0";
  progress.style.transform = `scaleX(${tab.isLoading ? Math.max(tab.progress, 0.05) : 1})`;
}

function formatBytes(bytes) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Math.max(bytes, 0);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 || value >= 100 ? 0 : 1)} ${units[unit]}`;
}

function downloadStatus(item) {
  switch (item.state) {
    case "complete":
      return formatBytes(item.receivedBytes);
    case "cancelled":
      return "Cancelled";
    case "failed":
      return "Failed";
    default: {
      const total = item.totalBytes > 0 ? ` of ${formatBytes(item.totalBytes)}` : "";
      const speed = item.bytesPerSecond > 0 ? ` · ${formatBytes(item.bytesPerSecond)}/s` : "";
      return `${formatBytes(item.receivedBytes)}${total}${speed}`;
    }
  }
}

function actionButton(label, icon, command) {
  const button = document.createElement("button");
  button.className = "icon-button download-action";
  button.title = label;
  button.setAttribute("aria-label", label);
  button.innerHTML = icon;
  button.addEventListener("click", () => send(command));
  return button;
}

function renderDownloads(items) {
  byId("downloads").hidden = items.length === 0;
  byId("clear-downloads").hidden = !items.some((item) => item.state !== "inProgress");

  // File names come from websites, so they're only ever set as text.
  const elements = items.map((item) => {
    const element = document.createElement("div");
    element.className = "download";

    const text = document.createElement("span");
    text.className = "download-text";
    const name = document.createElement("span");
    name.className = "download-name";
    name.textContent = item.fileName;
    name.title = item.fileName;
    const status = document.createElement("span");
    status.className = "download-status";
    status.textContent = downloadStatus(item);
    text.append(name, status);
    element.append(text);

    const id = item.id;
    if (item.state === "inProgress") {
      element.append(actionButton("Cancel download", CLOSE_ICON, { type: "cancelDownload", id }));
      const bar = document.createElement("div");
      bar.className = "download-progress";
      const fraction = item.totalBytes > 0 ? item.receivedBytes / item.totalBytes : 0.05;
      bar.style.transform = `scaleX(${Math.min(fraction, 1)})`;
      element.append(bar);
    } else {
      if (item.state === "complete") {
        element.append(actionButton("Show in folder", FOLDER_ICON, { type: "showDownload", id }));
      }
      element.append(actionButton("Remove from list", CLOSE_ICON, { type: "dismissDownload", id }));
    }
    return element;
  });
  byId("download-list").replaceChildren(...elements);
}

const findInput = byId("find-input");

function runFind(forward, again) {
  send({ type: "find", text: findInput.value, forward, again });
}

window.onBrowserEvent = (event) => {
  if (event.type === "showFind") {
    byId("find").hidden = false;
    findInput.focus();
    findInput.select();
  } else if (event.type === "hideFind") {
    byId("find").hidden = true;
    byId("find-count").textContent = "";
    findInput.classList.remove("no-match");
  } else if (event.type === "findResult") {
    const hasText = findInput.value.length > 0;
    byId("find-count").textContent = !hasText
      ? ""
      : event.count === 0
        ? "No matches"
        : event.active > 0
          ? `${event.active} of ${event.count}`
          : `${event.count} matches`;
    findInput.classList.toggle("no-match", hasText && event.count === 0);
  } else if (event.type === "downloads") {
    renderDownloads(event.items);
  } else if (event.type === "tabs") {
    const switched = event.strip.activeId !== strip.activeId;
    strip = event.strip;
    if (switched && document.activeElement === address) {
      address.blur();
    }
    render();
  } else if (event.type === "focusAddress") {
    address.focus();
    address.select();
  } else if (event.type === "keepAddressFocus") {
    address.focus();
  }
};

tabsElement.addEventListener("pointermove", continueDrag);
tabsElement.addEventListener("pointerup", finishDrag);
tabsElement.addEventListener("pointercancel", finishDrag);

findInput.addEventListener("input", () => runFind(true, false));
findInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    runFind(!event.shiftKey, true);
  }
});
byId("find-previous").addEventListener("click", () => runFind(false, true));
byId("find-next").addEventListener("click", () => runFind(true, true));
byId("find-close").addEventListener("click", () => send({ type: "closeFind" }));

byId("star").addEventListener("click", () => send({ type: "toggleBookmark" }));
for (const page of ["bookmarks", "history", "settings"]) {
  byId(`open-${page}`).addEventListener("click", () => send({ type: "openPage", page }));
}
byId("clear-downloads").addEventListener("click", () => send({ type: "clearDownloads" }));
byId("new-tab").addEventListener("click", () => send({ type: "newTab" }));
back.addEventListener("click", () => send({ type: "back" }));
forward.addEventListener("click", () => send({ type: "forward" }));
reload.addEventListener("click", () => {
  send({ type: activeTab()?.isLoading ? "stop" : "reload" });
});

byId("address-form").addEventListener("submit", (event) => {
  event.preventDefault();
  send({ type: "navigate", input: address.value });
  address.blur();
});

address.addEventListener("focus", () => address.select());
address.addEventListener("blur", render);
address.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    address.value = shownUrl(activeTab()?.url ?? "");
    address.select();
  }
});

send({ type: "ready" });
