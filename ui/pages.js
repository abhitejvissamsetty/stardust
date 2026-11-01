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

function refresh() {
  if (page === "settings") {
    send({ type: "getSettings" });
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
    showList(event.items.map((item) =>
      row(item, "Remove bookmark", { type: "removeBookmark", url: item.url })));
  } else if (event.type === "settings") {
    byId("engine").value = event.settings.searchEngine;
    byId("startup").value = event.settings.startup;
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
} else {
  byId("search").addEventListener("input", refresh);
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
