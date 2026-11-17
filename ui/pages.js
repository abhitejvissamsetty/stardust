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

// -- Extensions ----------------------------------------------------------------------
// Everything about an extension, such as its name and what it asks for, is written by
// whoever made it, so it's only ever set as text.

function el(tag, className, text) {
  const element = document.createElement(tag);
  if (className) {
    element.className = className;
  }
  if (text !== undefined) {
    element.textContent = text;
  }
  return element;
}

// A picture for an extension, or its first letter on a colour made from its name.
function extensionIcon(name, iconData) {
  if (iconData) {
    const image = el("img", "ext-icon");
    image.alt = "";
    image.src = iconData;
    return image;
  }
  let hash = 0;
  for (const char of name) {
    hash = (hash * 31 + char.codePointAt(0)) % 360;
  }
  const letter = el("div", "ext-icon fallback", [...name.trim()][0]?.toUpperCase() ?? "?");
  letter.style.setProperty("--hue", String(hash));
  letter.setAttribute("aria-hidden", "true");
  return letter;
}

const extensionsState = { pendingToken: null, removeAsk: null };

function warningChips(extension) {
  const chips = [];
  if (extension.allSites) {
    chips.push(el("span", "chip warn", "Can read all websites"));
  } else if (extension.hostPatterns.length > 0) {
    chips.push(el("span", "chip", `Can read ${extension.hostPatterns.length} site pattern${extension.hostPatterns.length === 1 ? "" : "s"}`));
  }
  if (extension.permissions.includes("declarativeNetRequest")) {
    chips.push(el("span", "chip", "Blocks content"));
  }
  if (extension.permissions.includes("cookies")) {
    chips.push(el("span", "chip", "Cookies"));
  }
  if (extension.permissions.includes("history") || extension.permissions.includes("tabs")) {
    chips.push(el("span", "chip", "Browsing"));
  }
  return chips;
}

function extensionCard(extension) {
  const card = el("li", extension.enabled ? "ext-card" : "ext-card off");
  card.dataset.id = extension.id;

  const top = el("div", "ext-top");
  top.append(extensionIcon(extension.name, extension.iconData));
  const heading = el("div", "ext-heading");
  const title = el("div", "ext-title");
  title.append(el("span", "ext-name", extension.name), el("span", "ext-version", `v${extension.version}`));
  heading.append(title, el("p", "ext-description", extension.description || "No description."));
  top.append(heading);

  const toggle = el("label", "switch");
  const box = document.createElement("input");
  box.type = "checkbox";
  box.setAttribute("role", "switch");
  box.checked = extension.enabled;
  box.setAttribute("aria-label", `${extension.enabled ? "Turn off" : "Turn on"} ${extension.name}`);
  box.addEventListener("change", () => send({ type: "setExtensionEnabled", id: extension.id, enabled: box.checked }));
  toggle.append(box, el("span", "slider"));
  top.append(toggle);
  card.append(top);

  const chips = el("div", "chips");
  chips.append(...warningChips(extension));
  if (extension.popupUrl) {
    chips.append(el("span", "chip good", "Has a popup"));
  }
  if (chips.childElementCount > 0) {
    card.append(chips);
  }

  const details = el("div", "ext-details");
  details.hidden = true;
  details.append(el("h3", "", "It can:"));
  const can = el("ul", "permissions");
  const warnings = extension.warnings.length > 0 ? extension.warnings : ["Work without asking for access to anything sensitive"];
  can.append(...warnings.map((warning) => el("li", "", warning)));
  details.append(can);
  const facts = el("dl", "facts");
  const fact = (label, value) => facts.append(el("dt", "", label), el("dd", "", value));
  fact("ID", extension.id);
  fact("Manifest", `version ${extension.manifestVersion}`);
  if (extension.permissions.length > 0) {
    fact("Permissions", extension.permissions.join(", "));
  }
  if (extension.hostPatterns.length > 0) {
    const shown = extension.hostPatterns.slice(0, 12).join(", ");
    const more = extension.hostPatterns.length - 12;
    fact("Sites", more > 0 ? `${shown}, and ${more} more` : shown);
  }
  details.append(facts);
  card.append(details);

  const actions = el("div", "ext-actions");
  const detailsButton = el("button", "row-action", "Details");
  detailsButton.setAttribute("aria-expanded", "false");
  detailsButton.addEventListener("click", () => {
    details.hidden = !details.hidden;
    detailsButton.setAttribute("aria-expanded", String(!details.hidden));
  });
  const pin = el("button", "row-action", extension.pinned ? "Unpin from toolbar" : "Pin to toolbar");
  pin.addEventListener("click", () => send({ type: "setExtensionPinned", id: extension.id, pinned: !extension.pinned }));
  const remove = el("button", "row-action danger", "Remove");
  remove.addEventListener("click", () => {
    // The first click asks, so a slip can't delete an extension.
    if (extensionsState.removeAsk === extension.id) {
      extensionsState.removeAsk = null;
      send({ type: "removeExtension", id: extension.id });
    } else {
      extensionsState.removeAsk = extension.id;
      remove.textContent = "Really remove?";
      setTimeout(() => {
        if (extensionsState.removeAsk === extension.id) {
          extensionsState.removeAsk = null;
          remove.textContent = "Remove";
        }
      }, 4000);
    }
  });
  actions.append(detailsButton, pin, remove);
  card.append(actions);
  return card;
}

function showConfirm(event) {
  const extension = event.extension;
  extensionsState.pendingToken = event.token;
  byId("confirm-title").textContent = extension.name;
  byId("confirm-version").textContent = `Version ${extension.version}`;
  byId("confirm-description").textContent = extension.description;
  byId("confirm-description").hidden = !extension.description;
  const icon = byId("confirm-icon");
  icon.hidden = !event.iconData;
  if (event.iconData) {
    icon.src = event.iconData;
  }
  const fallback = byId("confirm-fallback");
  fallback.hidden = Boolean(event.iconData);
  fallback.textContent = [...extension.name.trim()][0]?.toUpperCase() ?? "?";
  const warnings = extension.warnings.length > 0 ? extension.warnings : ["Work without asking for access to anything sensitive"];
  byId("confirm-permissions").replaceChildren(...warnings.map((warning) => el("li", extension.allSites && warning.includes("all websites") ? "bad" : "", warning)));
  byId("confirm").hidden = false;
  byId("confirm-add").focus();
}

function hideConfirm() {
  extensionsState.pendingToken = null;
  byId("confirm").hidden = true;
}

function extensionsEvent(event) {
  if (event.type === "extensionToInstall") {
    byId("error-banner").hidden = true;
    showConfirm(event);
  } else if (event.type === "extensionError") {
    byId("error-text").textContent = event.message;
    byId("error-banner").hidden = false;
  } else if (event.type === "extensions") {
    byId("list").replaceChildren(...event.extensions.map(extensionCard));
    byId("empty").hidden = event.extensions.length > 0;
    byId("restart-banner").hidden = !event.restartNeeded;
  }
}

function setupExtensions() {
  byId("add-folder").addEventListener("click", () => send({ type: "chooseExtension", source: "folder" }));
  byId("add-file").addEventListener("click", () => send({ type: "chooseExtension", source: "file" }));
  byId("restart").addEventListener("click", () => send({ type: "restartBrowser" }));
  byId("error-dismiss").addEventListener("click", () => {
    byId("error-banner").hidden = true;
  });
  byId("confirm-cancel").addEventListener("click", () => {
    send({ type: "cancelInstallExtension", token: extensionsState.pendingToken });
    hideConfirm();
  });
  byId("confirm-add").addEventListener("click", () => {
    send({ type: "confirmInstallExtension", token: extensionsState.pendingToken });
    hideConfirm();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !byId("confirm").hidden) {
      byId("confirm-cancel").click();
    }
  });
}

function refresh() {
  if (page === "extensions") {
    send({ type: "listExtensions" });
    return;
  }
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
  if (event.type === "extensions" || event.type === "extensionToInstall" || event.type === "extensionError") {
    extensionsEvent(event);
  } else if (event.type === "history") {
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
  } else if (event.type === "logins") {
    const row = (text, label, command) => {
      const element = document.createElement("div");
      element.className = "toolbar-row";
      const span = document.createElement("span");
      span.textContent = text;
      const button = document.createElement("button");
      button.className = "quiet-button";
      button.textContent = label;
      button.addEventListener("click", () => send(command));
      element.append(span, button);
      return element;
    };
    const logins = event.logins.map((login) =>
      row(`${login.origin} — ${login.username || "(no username)"}`, "Remove", {
        type: "deleteLogin",
        origin: login.origin,
        username: login.username,
      }),
    );
    byId("logins").replaceChildren(...logins);
    byId("logins-hint").hidden = logins.length > 0;
    const never = event.neverSave.map((origin) =>
      row(`Never saved: ${origin}`, "Allow saving", { type: "forgetNeverSaveLogin", origin }),
    );
    byId("never-save").replaceChildren(...never);
  } else if (event.type === "sitePermissions") {
    const names = { camera: "Camera", microphone: "Microphone", location: "Location" };
    const rows = Object.entries(event.sites).map(([site, decisions]) => {
      const row = document.createElement("div");
      row.className = "toolbar-row";
      const text = document.createElement("span");
      const summary = Object.entries(decisions)
        .map(([permission, decision]) => `${names[permission] ?? permission}: ${decision === "allow" ? "allowed" : "blocked"}`)
        .join(", ");
      text.textContent = `${site} — ${summary}`;
      const forget = document.createElement("button");
      forget.className = "quiet-button";
      forget.textContent = "Reset";
      forget.addEventListener("click", () => send({ type: "forgetSitePermissions", site }));
      row.append(text, forget);
      return row;
    });
    byId("site-permissions").replaceChildren(...rows);
    byId("site-permissions-hint").hidden = rows.length > 0;
  } else if (event.type === "updateStatus") {
    const status = event.status;
    const messages = {
      off: "Updates aren't set up in this build.",
      idle: "Updates are checked automatically.",
      checking: "Checking for updates…",
      upToDate: "Stardust is up to date.",
      downloading: `Downloading version ${status.version}…`,
      ready: `Version ${status.version} is ready. It installs when Stardust restarts.`,
      failed: `Couldn't update: ${status.message}`,
    };
    byId("update-status").textContent = `Version ${event.version}. ${messages[status.state] ?? ""}`;
    byId("check-updates").hidden = status.state === "off";
    byId("check-updates").disabled = status.state === "checking" || status.state === "downloading";
    byId("restart-update").hidden = status.state !== "ready";
    if (status.state === "checking" || status.state === "downloading") {
      setTimeout(() => send({ type: "getUpdateStatus" }), 1000);
    }
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
  send({ type: "getUpdateStatus" });
  send({ type: "listSitePermissions" });
  send({ type: "listLogins" });
  byId("check-updates").addEventListener("click", () => {
    send({ type: "checkForUpdates" });
    setTimeout(() => send({ type: "getUpdateStatus" }), 300);
  });
  byId("restart-update").addEventListener("click", () => send({ type: "restartToUpdate" }));
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
} else if (page === "extensions") {
  setupExtensions();
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
