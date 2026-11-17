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

const OPEN_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5" /></svg>';
const FOLDER_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 3h8a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>';

let strip = { tabs: [], activeId: null };
// The two tabs shown side by side, if any.
let splitIds = [];
let shownActiveId = null;
let windowId = 0;
let selectOnClick = false;
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

const GLOBE_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M3.5 12h17M12 3.5c2.5 2.4 3.5 5.2 3.5 8.5s-1 6.1-3.5 8.5c-2.5-2.4-3.5-5.2-3.5-8.5s1-6.1 3.5-8.5z" /></svg>';
const SPEAKER_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z" /><path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" /></svg>';
const MUTED_ICON =
  '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5z" /><path d="M16 9.5l5 5M21 9.5l-5 5" /></svg>';

// Page icons arrive once each, as pictures the browser has already fetched.
const favicons = new Map();

function createTabElement(id) {
  const element = document.createElement("div");
  element.className = "tab";
  element.dataset.id = id;
  element.setAttribute("role", "tab");
  element.tabIndex = 0;

  const icon = document.createElement("span");
  icon.className = "tab-icon";
  const spinner = document.createElement("span");
  spinner.className = "tab-spinner";
  const picture = document.createElement("img");
  picture.className = "tab-favicon";
  picture.alt = "";
  const fallback = document.createElement("span");
  fallback.className = "tab-fallback";
  fallback.innerHTML = GLOBE_ICON;
  icon.append(spinner, picture, fallback);
  const audio = document.createElement("button");
  audio.className = "icon-button tab-audio";
  audio.tabIndex = -1;
  audio.hidden = true;
  const title = document.createElement("span");
  title.className = "tab-title";
  const close = document.createElement("button");
  close.className = "icon-button tab-close";
  close.title = "Close tab";
  close.setAttribute("aria-label", "Close tab");
  close.tabIndex = -1;
  close.innerHTML = CLOSE_ICON;
  element.append(icon, title, audio, close);

  audio.addEventListener("pointerdown", (event) => event.stopPropagation());
  audio.addEventListener("click", (event) => {
    event.stopPropagation();
    send({ type: "toggleMuteTab", id });
  });
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
  element.draggable = true;
  element.addEventListener("pointerdown", (event) => {
    if (event.button === 0) {
      send({ type: "selectTab", id });
    }
  });
  element.addEventListener("dragstart", (event) => startDrag(event, id, element));
  element.addEventListener("dragend", finishDrag);
  return element;
}

// Dragging uses native drag and drop, the only events that keep working once the
// pointer leaves this toolbar's view.
const TAB_DRAG_TYPE = "application/x-stardust-tab";
const TEAR_OFF_DELAY_MS = 200;

function startDrag(event, id, element) {
  drag = { id, element, dropped: false };
  element.classList.add("dragging");
  event.dataTransfer.effectAllowed = "move";
  event.dataTransfer.setData(TAB_DRAG_TYPE, JSON.stringify({ windowId, id }));
}

function isTabDrag(event) {
  return event.dataTransfer.types.includes(TAB_DRAG_TYPE);
}

function dragOverStrip(event) {
  if (!isTabDrag(event)) {
    return;
  }
  event.preventDefault();
  event.dataTransfer.dropEffect = "move";
  if (!drag) {
    return;
  }
  const others = [...tabsElement.children].filter((child) => child !== drag.element);
  const before = others.find((child) => {
    const box = child.getBoundingClientRect();
    return event.clientX < box.left + box.width / 2;
  });
  tabsElement.insertBefore(drag.element, before || null);
}

function dropOnStrip(event) {
  if (!isTabDrag(event)) {
    return;
  }
  event.preventDefault();
  if (drag) {
    drag.dropped = true;
    send({ type: "moveTab", id: drag.id, index: [...tabsElement.children].indexOf(drag.element) });
    return;
  }
  // It came from another window. Only ever a pair of numbers, whatever was dragged.
  const data = JSON.parse(event.dataTransfer.getData(TAB_DRAG_TYPE));
  send({ type: "adoptTab", fromWindow: Number(data.windowId), id: Number(data.id) });
}

function finishDrag(event) {
  if (!drag) {
    return;
  }
  const { id, element, dropped } = drag;
  drag = null;
  element.classList.remove("dragging");
  const row = document.querySelector(".tab-row").getBoundingClientRect();
  const inRow = event.clientX >= row.left && event.clientX < row.right
    && event.clientY >= row.top && event.clientY < row.bottom;
  if (dropped) {
    // The strip already handled it.
  } else if (inRow) {
    send({ type: "moveTab", id, index: [...tabsElement.children].indexOf(element) });
  } else {
    // Wait a moment, so another window's strip can claim it first. If one does, this
    // tab has already left and the request does nothing.
    setTimeout(() => send({ type: "tearOffTab", id }), TEAR_OFF_DELAY_MS);
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
    element.classList.toggle("discarded", Boolean(tab.discarded));
    element.toggleAttribute("data-split", splitIds.includes(tab.id));
    // The spinner takes the icon's place while loading.
    const picture = element.querySelector(".tab-favicon");
    const data = tab.favicon ? favicons.get(tab.favicon) : null;
    element.querySelector(".tab-spinner").hidden = !tab.isLoading;
    picture.hidden = tab.isLoading || !data;
    if (data && picture.getAttribute("src") !== data) {
      picture.src = data;
    }
    element.querySelector(".tab-fallback").hidden = tab.isLoading || Boolean(data);
    const audio = element.querySelector(".tab-audio");
    audio.hidden = !tab.audible && !tab.muted;
    audio.innerHTML = tab.muted ? MUTED_ICON : SPEAKER_ICON;
    audio.title = tab.muted ? "Unmute tab" : "Mute tab";
    audio.setAttribute("aria-label", audio.title);
    tabsElement.append(element);
  }
  for (const element of existing.values()) {
    element.remove();
  }
  if (strip.activeId !== shownActiveId) {
    shownActiveId = strip.activeId;
    tabsElement.querySelector('[aria-selected="true"]')?.scrollIntoView({ inline: "nearest", block: "nearest" });
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

  renderSiteInfo(tab);
  const zoom = tab.zoomPercent || 100;
  const chip = byId("zoom-chip");
  chip.hidden = zoom === 100;
  chip.textContent = `${zoom}%`;
  if (menuState.owner === "menu" && menuState.zoom !== zoom) {
    openAppMenu();
  }
  byId("fill-login").hidden = !tab.hasLogins;
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
        const open = actionButton("Open", OPEN_ICON, { type: "openDownload", id, confirmed: false });
        if (item.isRisky) {
          // This file type could run code, so ask before opening it.
          const risky = open.cloneNode(true);
          risky.addEventListener("click", () => {
            if (confirm(`"${item.fileName}" could change your computer if it runs. Open it only if you trust where it came from. Open it?`)) {
              send({ type: "openDownload", id, confirmed: true });
            }
          });
          element.append(risky);
        } else {
          element.append(open);
        }
        element.append(actionButton("Show in folder", FOLDER_ICON, { type: "showDownload", id }));
      }
      element.append(actionButton("Remove from list", CLOSE_ICON, { type: "dismissDownload", id }));
    }
    return element;
  });
  byId("download-list").replaceChildren(...elements);
}

const findInput = byId("find-input");

// Page dialogs. The message and origin come from the page, so they're only set as text.
function showDialog(event) {
  const isLeave = event.kind === "leave";
  // For a permission, the browser sends the site's host and what it asks for.
  const isPermission = event.kind === "permission";
  let origin = "";
  try {
    origin = event.origin ? `${new URL(event.origin).host} says` : "";
  } catch {
    origin = event.origin;
  }
  byId("dialog-origin").textContent = isLeave ? "Leave this site?" : isPermission ? event.origin : origin;
  const message = isLeave ? "Changes you made may not be saved." : event.message;
  byId("dialog-message").textContent = message;
  byId("dialog-message").title = message;
  byId("dialog-cancel").hidden = event.kind === "alert";
  const isSave = event.kind === "savePassword";
  if (isSave) {
    byId("dialog-origin").textContent = event.origin;
  }
  byId("dialog-cancel").textContent = isLeave ? "Stay" : isPermission ? "Block" : isSave ? "Never" : "Cancel";
  byId("dialog-ok").textContent = isLeave ? "Leave" : isPermission ? "Allow" : isSave ? "Save" : "OK";
  const input = byId("dialog-input");
  input.hidden = event.kind !== "prompt";
  input.value = event.defaultText;
  byId("dialog").hidden = false;
  if (event.kind === "prompt") {
    input.focus();
    input.select();
  } else {
    byId("dialog-ok").focus();
  }
}

function answerDialog(accepted) {
  byId("dialog").hidden = true;
  send({ type: "answerDialog", accepted, input: byId("dialog-input").value });
}

const POPUP_ROW = 36;
let suggestions = [];
let selectedSuggestion = -1;
let typedQuery = "";
// The address bar is a command bar while its text starts with ">".
let commandMode = false;

function showPopupUnder(element, items, selected, minimumWidth = 0) {
  const box = element.getBoundingClientRect();
  send({
    type: "showPopup",
    left: Math.round(box.left),
    top: Math.round(box.bottom) + 2,
    width: Math.max(Math.round(box.width), minimumWidth),
    height: items.length * POPUP_ROW + 10,
    contents: { type: "items", items, selected },
  });
}

function hideSuggestions() {
  commandMode = false;
  if (suggestions.length > 0) {
    suggestions = [];
    selectedSuggestion = -1;
    send({ type: "hidePopup" });
  }
}

function renderBookmarksBar(bookmarks, folders) {
  const bar = byId("bookmarks-bar");
  const button = (label, title, onClick, className) => {
    const element = document.createElement("button");
    element.className = className;
    // Titles come from websites, so they're only ever set as text.
    element.textContent = label;
    element.title = title;
    element.addEventListener("click", onClick);
    return element;
  };
  const folderButtons = folders.map((folder) =>
    button(folder.name, folder.name, (event) => {
      if (folder.items.length > 0) {
        event.stopPropagation();
        showPopupUnder(event.currentTarget, folder.items, -1, 320);
        menuState.owner = "folder";
      }
    }, "bookmark folder"));
  const bookmarkButtons = bookmarks.map((item) =>
    button(item.title || item.url, item.url, () => send({ type: "navigate", input: item.url }), "bookmark"));
  bar.replaceChildren(...folderButtons, ...bookmarkButtons);
  bar.hidden = folderButtons.length + bookmarkButtons.length === 0;
}

// Which menu under the toolbar is showing, if any: "folder", "menu", "extensions" or "workspaces".
let menuState = { owner: null, zoom: 100 };

function hideMenu() {
  if (menuState.owner) {
    menuState.owner = null;
    send({ type: "hidePopup" });
    byId("menu-button").setAttribute("aria-expanded", "false");
    byId("extensions-button").setAttribute("aria-expanded", "false");
    byId("workspace-button").setAttribute("aria-expanded", "false");
  }
}

// Anything clicked other than a menu's own button puts the menu away.
document.addEventListener("pointerdown", (event) => {
  if (!event.target.closest("#menu-button, #extensions-button, #workspace-button, .ext-button")) {
    hideMenu();
    // An extension's popup goes away too, whether or not one is showing.
    send({ type: "hideExtensionPopup" });
  }
}, true);
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && menuState.owner) {
    event.preventDefault();
    hideMenu();
  }
});

let windowState = { maximized: false, minimized: false, fullscreen: false };
const matchCaseButton = byId("find-match-case");

function runFind(forward, again) {
  const matchCase = matchCaseButton.getAttribute("aria-pressed") === "true";
  send({ type: "find", text: findInput.value, forward, again, matchCase });
}

window.onBrowserEvent = (event) => {
  if (event.type === "suggestions") {
    if (event.query !== address.value.trim() || document.activeElement !== address) {
      return;
    }
    suggestions = event.items;
    selectedSuggestion = -1;
    if (suggestions.length > 0) {
      showPopupUnder(byId("address-form"), suggestions, -1);
    } else {
      send({ type: "hidePopup" });
    }
  } else if (event.type === "showDialog") {
    showDialog(event);
  } else if (event.type === "hideDialog") {
    byId("dialog").hidden = true;
  } else if (event.type === "pageFocused") {
    hideSuggestions();
    hideMenu();
    address.blur();
  } else if (event.type === "bookmarksBar") {
    renderBookmarksBar(event.bookmarks, event.folders);
  } else if (event.type === "confirmClose") {
    byId("downloads").hidden = false;
    byId("close-warning").hidden = false;
    const count = event.downloads;
    byId("close-warning-text").textContent =
      `Closing will cancel ${count} ${count === 1 ? "download" : "downloads"}.`;
  } else if (event.type === "showFind") {
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
    splitIds = event.split || [];
    windowId = event.windowId;
    document.documentElement.toggleAttribute("data-private", Boolean(event.private));
    byId("private-badge").hidden = !event.private;
    // A private window belongs to no workspace.
    byId("workspace-button").hidden = Boolean(event.private);
    if (switched) {
      hideSuggestions();
      if (document.activeElement === address) {
        address.blur();
      }
    }
    render();
  } else if (event.type === "commandBar") {
    openCommandBar();
  } else if (event.type === "focusAddress") {
    address.focus();
    address.select();
  } else if (event.type === "keepAddressFocus") {
    address.focus();
  } else if (event.type === "favicon") {
    favicons.set(event.url, event.data);
    renderTabs();
  } else if (event.type === "extensions") {
    extensionsState = { items: event.items, available: event.available };
    renderPinned();
    if (menuState.owner === "extensions") {
      openExtensionsMenu();
    }
  } else if (event.type === "toast") {
    showToast(event.text);
  } else if (event.type === "sidePanel") {
    sidePanelOpen = event.open;
    byId("side-panel-button").setAttribute("aria-pressed", String(event.open));
  } else if (event.type === "layout") {
    tabLayout = { vertical: event.vertical, collapsed: event.collapsed };
    document.documentElement.toggleAttribute("data-vertical-tabs", event.vertical);
    if (menuState.owner === "menu") {
      openAppMenu();
    }
  } else if (event.type === "workspaces") {
    workspacesState = { items: event.items, current: event.current };
    renderWorkspace();
    if (menuState.owner === "workspaces") {
      openWorkspaceMenu();
    }
  } else if (event.type === "windowState") {
    windowState = event;
    document.documentElement.toggleAttribute("data-fullscreen", event.fullscreen);
  } else if (event.type === "menuReady") {
    openingMenu = false;
    focusBeforeMenu?.focus();
    focusBeforeMenu = null;
  }
};

// Moving focus to the page leaves this page's focused element in place, so end editing
// explicitly. Otherwise the address bar wouldn't follow the page.
window.addEventListener("blur", () => {
  if (openingMenu) {
    return;
  }
  hideSuggestions();
  address.blur();
});

tabsElement.addEventListener("wheel", (event) => {
  event.preventDefault();
  tabsElement.scrollLeft += event.deltaY || event.deltaX;
}, { passive: false });
// The whole row takes drops, including the empty space after the last tab.
const tabRow = document.querySelector(".tab-row");
tabRow.addEventListener("dragover", dragOverStrip);
tabRow.addEventListener("drop", dropOnStrip);

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
matchCaseButton.addEventListener("click", () => {
  const pressed = matchCaseButton.getAttribute("aria-pressed") !== "true";
  matchCaseButton.setAttribute("aria-pressed", String(pressed));
  findInput.focus();
  runFind(true, false);
});

byId("dialog-ok").addEventListener("click", () => answerDialog(true));
byId("dialog-cancel").addEventListener("click", () => answerDialog(false));
byId("dialog").addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    answerDialog(false);
  } else if (event.key === "Enter" && event.target === byId("dialog-input")) {
    event.preventDefault();
    answerDialog(true);
  }
});

byId("star").addEventListener("click", () => send({ type: "toggleBookmark" }));
byId("fill-login").addEventListener("click", () => send({ type: "fillLogin" }));
byId("keep-downloading").addEventListener("click", () => {
  byId("close-warning").hidden = true;
  send({ type: "confirmClose", close: false });
});
byId("close-anyway").addEventListener("click", () => {
  byId("close-warning").hidden = true;
  send({ type: "confirmClose", close: true });
});
byId("menu-button").addEventListener("click", () => toggleMenu("menu", openAppMenu));
byId("extensions-button").addEventListener("click", () => toggleMenu("extensions", openExtensionsMenu));
byId("workspace-button").addEventListener("click", () => toggleMenu("workspaces", openWorkspaceMenu));
byId("side-panel-button").addEventListener("click", () => {
  hideMenu();
  send({ type: "toggleSidePanel" });
});
byId("zoom-chip").addEventListener("click", () => send({ type: "zoomReset" }));
byId("clear-downloads").addEventListener("click", () => send({ type: "clearDownloads" }));
byId("new-tab").addEventListener("click", () => send({ type: "newTab" }));
back.addEventListener("click", () => send({ type: "back" }));
forward.addEventListener("click", () => send({ type: "forward" }));
reload.addEventListener("click", () => {
  send({ type: activeTab()?.isLoading ? "stop" : "reload" });
});

byId("address-form").addEventListener("submit", (event) => {
  event.preventDefault();
  if (commandMode) {
    runCommandResult();
    return;
  }
  hideSuggestions();
  send({ type: "navigate", input: address.value });
  address.blur();
});

address.addEventListener("input", () => {
  typedQuery = address.value;
  if (address.value.startsWith(">")) {
    showCommandResults();
    return;
  }
  commandMode = false;
  const query = address.value.trim();
  if (query) {
    send({ type: "suggest", query });
  } else {
    hideSuggestions();
  }
});

// Focus can come back to the address bar at any time, so selecting all its text only
// happens on purpose: on the shortcut, or the first click into it.
address.addEventListener("pointerdown", () => {
  selectOnClick = document.activeElement !== address;
});
address.addEventListener("click", () => {
  if (selectOnClick && address.selectionStart === address.selectionEnd) {
    address.select();
  }
  selectOnClick = false;
});
address.addEventListener("blur", () => {
  if (openingMenu) {
    return;
  }
  render();
  // Clicking a suggestion moves focus first, so give the click time to arrive.
  setTimeout(() => {
    if (document.activeElement !== address) {
      hideSuggestions();
    }
  }, 200);
});
address.addEventListener("keydown", (event) => {
  if ((event.key === "ArrowDown" || event.key === "ArrowUp") && commandMode && suggestions.length > 0) {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    selectedSuggestion = (selectedSuggestion + step + suggestions.length) % suggestions.length;
    showPopupUnder(byId("address-form"), suggestions, selectedSuggestion);
  } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && suggestions.length > 0) {
    event.preventDefault();
    const step = event.key === "ArrowDown" ? 1 : -1;
    const positions = suggestions.length + 1;
    selectedSuggestion = ((selectedSuggestion + 1 + step + positions) % positions) - 1;
    address.value = selectedSuggestion >= 0 ? suggestions[selectedSuggestion].url : typedQuery;
    showPopupUnder(byId("address-form"), suggestions, selectedSuggestion);
  } else if (event.key === "Escape" && commandMode) {
    hideSuggestions();
    address.value = shownUrl(activeTab()?.url ?? "");
    address.select();
  } else if (event.key === "Escape") {
    if (suggestions.length > 0) {
      hideSuggestions();
      address.value = typedQuery;
    } else {
      address.value = shownUrl(activeTab()?.url ?? "");
      address.select();
    }
  }
});


// -- Page security, shown beside the address ---------------------------------------------

function renderSiteInfo(tab) {
  const info = byId("site-info");
  const secure = tab.url.startsWith("https://");
  let host = "";
  try {
    host = new URL(tab.url).hostname;
  } catch {
    host = "";
  }
  // Plain HTTP to this computer is for developing sites, so it isn't a warning.
  const local = host === "localhost" || host === "127.0.0.1" || host.endsWith(".localhost");
  const insecure = tab.url.startsWith("http://") && !local;
  info.hidden = !secure && !insecure;
  info.classList.toggle("warning", insecure);
  byId("lock-icon").toggleAttribute("hidden", !secure);
  byId("warn-icon").toggleAttribute("hidden", !insecure);
  byId("site-info-label").textContent = insecure ? "Not secure" : "";
  info.title = secure ? "Connection is secure" : insecure ? "This page isn't encrypted, so others on the network could see it" : "";
}

// A short message beside the address bar, such as "Clipped".
let toastTimer = null;
function showToast(text) {
  const toast = byId("toast");
  toast.textContent = text;
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.hidden = true), 2200);
}

// -- Command bar --------------------------------------------------------------------------
//
// The address bar, switched to search what's open and what can be done, when its text
// starts with ">". Nothing here is sent to a search engine or a website.

const COMMAND_RESULTS = 8;

// Everything the bar can do, as {title, hint, command}. Built each time, so its labels
// match what's showing now.
function commandBarItems() {
  const page = (name) => ({ type: "openPage", page: name });
  const items = [];
  for (const tab of strip.tabs) {
    if (tab.id !== strip.activeId) {
      items.push({ title: tabLabel(tab), hint: "Tab", command: { type: "selectTab", id: tab.id }, group: 0 });
    }
  }
  if (workspacesState.items.length > 1 && !document.documentElement.hasAttribute("data-private")) {
    for (const item of workspacesState.items) {
      if (item.id !== workspacesState.current) {
        items.push({ title: `Workspace: ${item.name}`, hint: "Workspace", command: { type: "openWorkspace", id: item.id }, group: 1 });
      }
    }
  }
  const command = (title, hint, run) => items.push({ title, hint, command: run, group: 2 });
  command("New tab", shortcut("T"), { type: "newTab" });
  command("New window", shortcut("N"), { type: "newWindow" });
  command("New private window", shortcut("N", { shift: true }), { type: "newPrivateWindow" });
  command("Reopen closed tab", shortcut("T", { shift: true }), { type: "reopenClosedTab" });
  command("Close this tab", shortcut("W"), { type: "closeTab", id: strip.activeId });
  command("Clip this page", shortcut("S", { shift: true }), { type: "clipPage" });
  command("Pick a colour from the page", "Command", { type: "pickColor" });
  command("Identify fonts and colours of text", "Command", { type: "inspectText" });
  command("Screenshot: what's showing", "Command", { type: "screenshot", full: false });
  command("Screenshot: the whole page", "Command", { type: "screenshot", full: true });
  command(splitIds.length ? "Close split view" : "Split view", "Command", { type: "toggleSplitView" });
  command(sidePanelOpen ? "Close the side panel" : "Open the side panel", shortcut("E", { shift: true }), { type: "toggleSidePanel" });
  command(tabLayout.vertical ? "Show tabs across the top" : "Show tabs down the side", "Command", { type: "setVerticalTabs", enabled: !tabLayout.vertical });
  command("Reload", shortcut("R"), { type: "reload" });
  command("Find in page", shortcut("F"), { type: "openFind" });
  command("Bookmark this page", shortcut("D"), { type: "toggleBookmark" });
  command("Zoom in", shortcut("+"), { type: "zoomIn" });
  command("Zoom out", shortcut("-"), { type: "zoomOut" });
  command("Reset zoom", shortcut("0"), { type: "zoomReset" });
  command("Print", shortcut("P"), { type: "print" });
  command("Developer tools", isMac ? "⌥⌘I" : "F12", { type: "devTools" });
  command("Full screen", isMac ? "⌃⌘F" : "F11", { type: "windowAction", action: "toggleFullscreen" });
  command("History", shortcut("H"), page("history"));
  command("Downloads", shortcut("J"), page("downloads"));
  command("Bookmarks", shortcut("O", { shift: true }), page("bookmarks"));
  command("Library: clips, screenshots, colours", "Page", page("library"));
  command("Extensions", "Page", page("extensions"));
  command("Settings", shortcut(","), page("settings"));
  command("Quit Stardust", shortcut("Q"), { type: "quit" });
  return items;
}

// How well a query matches a title, or -1 for not at all. Starting with it beats starting
// a word with it, which beats containing it, which beats its letters appearing in order.
function matchScore(title, query) {
  const text = title.toLowerCase();
  const wanted = query.toLowerCase();
  if (!wanted) {
    return 0;
  }
  if (text.startsWith(wanted)) {
    return 100;
  }
  const at = text.indexOf(wanted);
  if (at > 0) {
    return /[\s:/.\-_]/.test(text[at - 1]) ? 80 : 60;
  }
  let next = 0;
  for (const char of wanted) {
    next = text.indexOf(char, next) + 1;
    if (next === 0) {
      return -1;
    }
  }
  return 20;
}

function commandBarResults(query) {
  const scored = commandBarItems()
    .map((item, order) => ({ item, order, score: matchScore(item.title, query) }))
    .filter((entry) => entry.score >= 0);
  // Best match first; ties keep tabs, then workspaces, then commands, as listed.
  scored.sort((a, b) => b.score - a.score || a.item.group - b.item.group || a.order - b.order);
  return scored.slice(0, COMMAND_RESULTS).map(({ item }) => ({ title: item.title, url: item.hint, command: item.command }));
}

function showCommandResults() {
  commandMode = true;
  suggestions = commandBarResults(address.value.slice(1).trim());
  // The first result is ready, so Enter does it.
  selectedSuggestion = suggestions.length > 0 ? 0 : -1;
  if (suggestions.length > 0) {
    showPopupUnder(byId("address-form"), suggestions, selectedSuggestion);
  } else {
    send({ type: "hidePopup" });
  }
}

function openCommandBar() {
  hideMenu();
  address.focus();
  address.value = "> ";
  typedQuery = address.value;
  address.setSelectionRange(address.value.length, address.value.length);
  showCommandResults();
}

function runCommandResult() {
  const result = suggestions[Math.max(selectedSuggestion, 0)];
  hideSuggestions();
  address.blur();
  if (result?.command) {
    send(result.command);
  }
}

// -- Menus -------------------------------------------------------------------------------

const isMac = /Mac/.test(navigator.platform);
const shortcut = (key, { shift = false, alt = false } = {}) =>
  isMac ? `${alt ? "⌥" : ""}${shift ? "⇧" : ""}⌘${key}` : `Ctrl+${shift ? "Shift+" : ""}${alt ? "Alt+" : ""}${key}`;

const ROW_HEIGHT = { item: 34, separator: 9, zoom: 40, extension: 40, note: 40, header: 30, workspace: 38 };
const MENU_WIDTH = 300;

let extensionsState = { items: [], available: true };
let workspacesState = { items: [], current: "" };
let tabLayout = { vertical: false, collapsed: false };
let sidePanelOpen = false;

function toggleMenu(owner, open) {
  if (menuState.owner === owner) {
    hideMenu();
    return;
  }
  hideSuggestions();
  open();
}

function showMenu(owner, button, rows, width) {
  // Only one thing hangs under the toolbar at a time.
  send({ type: "hideExtensionPopup" });
  const box = button.getBoundingClientRect();
  const height = rows.reduce((sum, row) => sum + ROW_HEIGHT[row.kind || "item"], 0) + 12;
  menuState = { owner, zoom: activeTab()?.zoomPercent || 100 };
  byId("menu-button").setAttribute("aria-expanded", String(owner === "menu"));
  byId("extensions-button").setAttribute("aria-expanded", String(owner === "extensions"));
  byId("workspace-button").setAttribute("aria-expanded", String(owner === "workspaces"));
  send({
    type: "showPopup",
    left: Math.round(Math.max(8, box.right - width)),
    top: Math.round(box.bottom) + 4,
    width,
    height,
    contents: { type: "menu", rows, private: document.documentElement.hasAttribute("data-private") },
  });
}

function openAppMenu() {
  const page = (name) => ({ type: "openPage", page: name });
  const rows = [
    { label: "New tab", shortcut: shortcut("T"), command: { type: "newTab" } },
    { label: "New window", shortcut: shortcut("N"), command: { type: "newWindow" } },
    { label: "New private window", shortcut: shortcut("N", { shift: true }), command: { type: "newPrivateWindow" } },
    { kind: "separator" },
    { label: "History", shortcut: shortcut("H"), command: page("history") },
    { label: "Downloads", shortcut: shortcut("J"), command: page("downloads") },
    { label: "Bookmarks", shortcut: shortcut("O", { shift: true }), command: page("bookmarks") },
    { label: "Library: clips, screenshots, colours", command: page("library") },
    { label: "Extensions", command: page("extensions") },
    { label: "Command bar: find tabs, workspaces, commands", shortcut: shortcut("A", { shift: true }), command: { type: "commandBar" } },
    { label: "Clip this page", shortcut: shortcut("S", { shift: true }), command: { type: "clipPage" } },
    { label: "Pick a colour from the page", command: { type: "pickColor" } },
    { label: "Identify fonts and colours of text", command: { type: "inspectText" } },
    { label: "Screenshot: what's showing", command: { type: "screenshot", full: false } },
    { label: "Screenshot: the whole page", command: { type: "screenshot", full: true } },
    { label: sidePanelOpen ? "Close the side panel" : "Side panel: notes, to-dos, snippets", shortcut: shortcut("E", { shift: true }), command: { type: "toggleSidePanel" } },
    { label: splitIds.length ? "Close split view" : "Split view: two pages side by side", command: { type: "toggleSplitView" } },
    { label: tabLayout.vertical ? "Show tabs across the top" : "Show tabs down the side", command: { type: "setVerticalTabs", enabled: !tabLayout.vertical } },
    { kind: "separator" },
    { kind: "zoom", percent: activeTab()?.zoomPercent || 100 },
    { kind: "separator" },
    { label: "Print…", shortcut: shortcut("P"), command: { type: "print" } },
    { label: "Find in page…", shortcut: shortcut("F"), command: { type: "openFind" } },
    { label: "Reopen closed tab", shortcut: shortcut("T", { shift: true }), command: { type: "reopenClosedTab" } },
    { label: "Developer tools", shortcut: isMac ? "⌥⌘I" : "F12", command: { type: "devTools" } },
    { kind: "separator" },
    { label: "Settings", shortcut: shortcut(","), command: page("settings") },
    { label: "Quit Stardust", shortcut: shortcut("Q"), command: { type: "quit" } },
  ];
  showMenu("menu", byId("menu-button"), rows, MENU_WIDTH);
}

function openWorkspaceMenu() {
  const rows = [{ kind: "header", label: "Workspaces" }];
  for (const item of workspacesState.items) {
    rows.push({ kind: "workspace", id: item.id, name: item.name, color: item.color, open: item.open, current: item.id === workspacesState.current });
  }
  rows.push({ kind: "separator" }, { label: "Manage workspaces", command: { type: "openPage", page: "settings" } });
  showMenu("workspaces", byId("workspace-button"), rows, MENU_WIDTH);
}

function renderWorkspace() {
  const current = workspacesState.items.find((item) => item.id === workspacesState.current) || workspacesState.items[0];
  const button = byId("workspace-button");
  if (!current) {
    return;
  }
  byId("ws-dot").dataset.color = current.color;
  byId("ws-name").textContent = current.name;
  button.title = `Workspace: ${current.name}. Switch workspace`;
}

function openExtensionsMenu() {
  const button = byId("extensions-button");
  const box = button.getBoundingClientRect();
  const rows = [{ kind: "header", label: "Extensions" }];
  if (!extensionsState.available) {
    rows.push({ kind: "note", label: "Extensions only run in the main workspace, not in private windows or other workspaces." });
  } else if (extensionsState.items.length === 0) {
    rows.push({ kind: "note", label: "No extensions yet." });
  } else {
    for (const item of extensionsState.items) {
      // A popup opens under the button, with its right edge there.
      rows.push({ kind: "extension", ...item, anchor: { left: Math.max(8, Math.round(box.right) - 380), top: Math.round(box.bottom) + 4 } });
    }
  }
  rows.push({ kind: "separator" }, { label: "Manage extensions", command: { type: "openPage", page: "extensions" } });
  showMenu("extensions", button, rows, MENU_WIDTH);
}

function renderPinned() {
  const container = byId("pinned-extensions");
  const buttons = extensionsState.items
    .filter((item) => item.pinned)
    .map((item) => {
      const button = document.createElement("button");
      button.className = "icon-button ext-button";
      const label = item.title || item.name;
      button.title = item.popupUrl ? label : `${label} (no popup: open Extensions to manage it)`;
      button.setAttribute("aria-label", label);
      if (item.iconData) {
        const image = document.createElement("img");
        image.alt = "";
        image.src = item.iconData;
        button.append(image);
      } else {
        let hue = 0;
        for (const char of item.name) {
          hue = (hue * 31 + char.codePointAt(0)) % 360;
        }
        const letter = document.createElement("span");
        letter.className = "ext-letter";
        letter.style.setProperty("--hue", String(hue));
        letter.textContent = [...item.name.trim()][0]?.toUpperCase() ?? "?";
        button.append(letter);
      }
      button.addEventListener("click", () => {
        hideMenu();
        if (item.popupUrl) {
          const box = button.getBoundingClientRect();
          send({ type: "openExtensionPopup", id: item.id, left: Math.max(8, Math.round(box.right) - 380), top: Math.round(box.bottom) + 4 });
        } else {
          send({ type: "openPage", page: "extensions" });
        }
      });
      return button;
    });
  container.replaceChildren(...buttons);
}

send({ type: "ready" });

// The menu under the toolbar, for suggestions and bookmark folders. Opening it from
// here keeps it in this page's process, and the browser shows it as an overlay. A page
// may only open a window when someone presses a key or clicks, so it opens on the first
// one, which is also the first time it can be needed.
const MENU_HTML = /*MENU*/"";
let menuWindow = null;
let focusBeforeMenu = null;
// The new menu takes focus for a moment, which isn't someone leaving the toolbar.
let openingMenu = false;

function openMenu() {
  if (menuWindow) {
    return;
  }
  focusBeforeMenu = document.activeElement;
  openingMenu = true;
  menuWindow = window.open("", "stardust-menu");
  if (menuWindow) {
    menuWindow.document.write(MENU_HTML);
    menuWindow.document.close();
  } else {
    openingMenu = false;
  }
}

window.addEventListener("keydown", openMenu, { capture: true });
window.addEventListener("pointerdown", openMenu, { capture: true });
