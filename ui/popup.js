"use strict";

// The toolbar opened this menu, and the menu speaks through the toolbar's bridge.
const send = (command) => window.opener.browserBridge.send(JSON.stringify(command));

// Titles and URLs come from websites, so they're only ever set as text.
window.onBrowserEvent = (event) => {
  if (event.type !== "items") {
    return;
  }
  const elements = event.items.map((item, index) => {
    const element = document.createElement("div");
    element.className = index === event.selected ? "item selected" : "item";
    element.setAttribute("role", "option");
    if (item.isBookmark) {
      const mark = document.createElement("span");
      mark.className = "mark";
      mark.textContent = "★";
      element.append(mark);
    }
    const title = document.createElement("span");
    title.className = "title";
    title.textContent = item.title || item.url;
    const url = document.createElement("span");
    url.className = "url";
    url.textContent = item.title ? item.url : "";
    element.append(title, url);
    element.addEventListener("mousedown", (click) => {
      click.preventDefault();
      send({ type: "navigate", input: item.url });
      send({ type: "hidePopup" });
    });
    return element;
  });
  document.getElementById("items").replaceChildren(...elements);
};

send({ type: "popupReady" });
