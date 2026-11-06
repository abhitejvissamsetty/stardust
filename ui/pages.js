"use strict";

const page = document.body.dataset.page;
const byId = (id) => document.getElementById(id);
const send = (command) => window.browserBridge.send(JSON.stringify(command));

const CLOSE_ICON = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>';

// Titles and URLs come from websites, so they're only ever set as text, and only web
// addresses become links.
function row(item, removeLabel, removeCommand, time) {
  const element = document.createElement("li");
  element.className = "row";

  const link = document.createElement("a");
  if (/^https?:\/\//i.test(item.url)) {
    link.href = item.url;
  }
  const title = document.createElement("span");
  title.className = "row-title";
  title.textContent = item.title || item.url;
  const url = document.createElement("span");
  url.className = "row-url";
  url.textContent = item.url;
  link.append(title, url);
  element.append(link);

  if (time) {
    const stamp = document.createElement("span");
    stamp.className = "row-time";
    stamp.textContent = new Date(time).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
    element.append(stamp);
  }

  const remove = document.createElement("button");
  remove.className = "row-remove";
  remove.title = removeLabel;
  remove.setAttribute("aria-label", removeLabel);
  remove.innerHTML = CLOSE_ICON;
  remove.addEventListener("click", () => {
    send(removeCommand);
    refresh();
  });
  element.append(remove);
  return element;
}

function showList(elements) {
  byId("list").replaceChildren(...elements);
  byId("empty").hidden = elements.length > 0;
}

let folders = [];

function actionButton(label, onClick) {
  const button = document.createElement("button");
  button.className = "row-action";
  button.textContent = label;
  button.addEventListener("click", onClick);
  return button;
}

function folderRow(folder) {
  const element = document.createElement("li");
  element.className = "row";
  const name = document.createElement("input");
  name.className = "row-name";
  name.value = folder.name;
  name.setAttribute("aria-label", "Folder name");
  name.addEventListener("change", () => {
    send({ type: "renameFolder", id: folder.id, name: name.value });
    refresh();
  });
  element.append(name, actionButton("Delete folder", () => {
    send({ type: "deleteFolder", id: folder.id });
    refresh();
  }));
  return element;
}

function bookmarkEditForm(item, rowElement) {
  const form = document.createElement("form");
  form.className = "edit-form";
  const title = document.createElement("input");
  title.value = item.title;
  title.setAttribute("aria-label", "Bookmark name");
  const url = document.createElement("input");
  url.value = item.url;
  url.setAttribute("aria-label", "Bookmark address");
  url.spellcheck = false;
  const folder = document.createElement("select");
  folder.setAttribute("aria-label", "Folder");
  const none = document.createElement("option");
  none.value = "";
  none.textContent = "No folder";
  folder.append(none, ...folders.map((entry) => {
    const option = document.createElement("option");
    option.value = entry.id;
    option.textContent = entry.name;
    return option;
  }));
  folder.value = item.folderId ?? "";
  const buttons = document.createElement("div");
  buttons.className = "toolbar-row button-row";
  const save = document.createElement("button");
  save.className = "quiet-button";
  save.textContent = "Save";
  buttons.append(save, actionButton("Cancel", (event) => {
    event.preventDefault();
    refresh();
  }));
  const error = document.createElement("span");
  error.className = "hint error";
  error.id = "edit-error";
  error.hidden = true;
  error.textContent = "Enter a web address that isn't already bookmarked.";
  form.append(title, url, folder, error, buttons);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    send({
      type: "updateBookmark",
      url: item.url,
      newUrl: url.value,
      title: title.value,
      folderId: folder.value === "" ? null : Number(folder.value),
    });
  });
  rowElement.replaceChildren(form);
  rowElement.className = "";
  title.focus();
}

function formatSize(bytes) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Math.max(bytes, 0);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 || value >= 100 ? 0 : 1)} ${units[unit]}`;
}

function downloadRow(item) {
  const element = document.createElement("li");
  element.className = item.exists ? "row" : "row missing";
  const text = document.createElement("div");
  text.className = "row-name";
  const title = document.createElement("span");
  title.className = "row-title";
  title.textContent = item.fileName;
  const status = document.createElement("span");
  status.className = "row-url";
  const when = new Date(item.finishedMs).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
  status.textContent = item.state === "failed"
    ? `Failed · ${when}`
    : item.exists ? `${formatSize(item.receivedBytes)} · ${when}` : `Deleted · ${when}`;
  text.append(title, status);
  element.append(text);
  if (item.exists && item.state === "complete") {
    element.append(actionButton("Open", () => {
      const needsAsking = item.isRisky;
      if (needsAsking && !confirm(`"${item.fileName}" could change your computer if it runs. Open it only if you trust where it came from. Open it?`)) {
        return;
      }
      send({ type: "openSavedDownload", path: item.path, confirmed: needsAsking });
    }), actionButton("Show in folder", () => send({ type: "showSavedDownload", path: item.path })));
  }
  const remove = document.createElement("button");
  remove.className = "row-remove";
  remove.title = "Remove from list";
  remove.setAttribute("aria-label", "Remove from list");
  remove.innerHTML = CLOSE_ICON;
  remove.addEventListener("click", () => {
    send({ type: "removeSavedDownload", path: item.path });
    refresh();
  });
  element.append(remove);
  return element;
}

function refresh() {
  if (page === "settings") {
    send({ type: "getSettings" });
    return;
  }
  if (page === "downloads") {
    send({ type: "listDownloads" });
    return;
  }
  const query = byId("search").value;
  send({ type: page === "history" ? "listHistory" : "listBookmarks", query });
}

window.onBrowserEvent = (event) => {
  if (event.type === "history") {
    showList(event.items.map((item) =>
      row(item, "Remove from history", { type: "deleteHistory", url: item.url }, item.lastVisitMs)));
  } else if (event.type === "bookmarks") {
    folders = event.folders;
    byId("folders").replaceChildren(...folders.map(folderRow));
    showList(event.items.map((item) => {
      const element = row(item, "Remove bookmark", { type: "removeBookmark", url: item.url });
      const folder = folders.find((entry) => entry.id === item.folderId);
      const remove = element.querySelector(".row-remove");
      if (folder) {
        const label = document.createElement("span");
        label.className = "row-folder";
        label.textContent = folder.name;
        element.insertBefore(label, remove);
      }
      element.insertBefore(actionButton("Edit", () => bookmarkEditForm(item, element)), remove);
      return element;
    }));
  } else if (event.type === "downloads") {
    showList(event.items.map(downloadRow));
  } else if (event.type === "dataCleared") {
    byId("clear-hint").textContent = "Cleared.";
  } else if (event.type === "bookmarkSaved") {
    if (event.saved) {
      refresh();
    } else {
      byId("edit-error").hidden = false;
    }
  } else if (event.type === "settings") {
    byId("engine").value = event.settings.searchEngine;
    byId("startup").value = event.settings.startup;
    byId("ask-download").checked = event.settings.askDownloadLocation;
    byId("save-memory").checked = event.settings.saveMemory;
    if (document.activeElement !== byId("homepage")) {
      byId("homepage").value = event.settings.homepage;
    }
  } else if (event.type === "importSources") {
    const select = byId("import-source");
    const options = event.sources.map((source) => {
      const option = document.createElement("option");
      option.value = source.id;
      option.textContent = source.name;
      return option;
    });
    select.replaceChildren(...options);
    select.disabled = options.length === 0;
    byId("import").disabled = options.length === 0;
    if (options.length === 0) {
      byId("import-hint").textContent = "No other browsers found on this computer.";
    }
  } else if (event.type === "importResult") {
    byId("import").disabled = false;
    byId("import-hint").textContent =
      `Added ${event.bookmarks} bookmarks and ${event.history} history entries.`;
  } else if (event.type === "homepageResult") {
    const hint = byId("homepage-hint");
    hint.classList.toggle("error", !event.saved);
    hint.textContent = event.saved
      ? "Saved. Used when there are no tabs to reopen."
      : "Enter a web address, such as example.com.";
  }
};

if (page === "settings") {
  byId("engine").addEventListener("change", (event) => {
    send({ type: "setSearchEngine", engine: event.target.value });
  });
  send({ type: "listImportSources" });
  byId("ask-download").addEventListener("change", (event) => {
    send({ type: "setAskDownloadLocation", ask: event.target.checked });
  });
  byId("save-memory").addEventListener("change", (event) => {
    send({ type: "setSaveMemory", enabled: event.target.checked });
  });
  byId("clear-data").addEventListener("click", () => {
    send({
      type: "clearData",
      history: byId("clear-history").checked,
      cookies: byId("clear-cookies").checked,
      cache: byId("clear-cache").checked,
      downloads: byId("clear-downloads").checked,
    });
  });
  byId("import").addEventListener("click", () => {
    byId("import").disabled = true;
    byId("import-hint").textContent = "Importing…";
    send({ type: "import", source: byId("import-source").value });
  });
  byId("startup").addEventListener("change", (event) => {
    send({ type: "setStartup", startup: event.target.value });
  });
  byId("homepage").addEventListener("change", (event) => {
    send({ type: "setHomepage", homepage: event.target.value });
  });
} else if (page === "downloads") {
  byId("clear").addEventListener("click", () => {
    send({ type: "clearSavedDownloads" });
    refresh();
  });
} else {
  byId("search").addEventListener("input", refresh);
}
if (page === "bookmarks") {
  byId("add-folder").addEventListener("click", () => {
    const name = byId("new-folder");
    send({ type: "createFolder", name: name.value });
    name.value = "";
    refresh();
  });
}
if (page === "history") {
  byId("clear").addEventListener("click", () => {
    if (confirm("Clear all browsing history?")) {
      send({ type: "clearHistory" });
      refresh();
    }
  });
}

refresh();
