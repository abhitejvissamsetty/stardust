"use strict";

// The toolbar opened this menu, and the menu speaks through the toolbar's bridge.
const send = (command) => window.opener.browserBridge.send(JSON.stringify(command));

const ICONS = {
  minus: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14" /></svg>',
  plus: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>',
  expand: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" /></svg>',
  pin: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 4h6l-1 6 3 3H7l3-3zM12 13v7" /></svg>',
};

function el(tag, className, text) {
  const element = document.createElement(tag);
  element.className = className ?? "";
  if (text !== undefined) {
    element.textContent = text;
  }
  return element;
}

// Rows act on mouse down, as the suggestions do, so that taking focus can't undo a click.
function onPress(element, action) {
  element.addEventListener("mousedown", (event) => {
    event.preventDefault();
    event.stopPropagation();
    action();
  });
}

// The toolbar's menus. Labels, and an extension's name, are only ever set as text.
function renderMenu(event) {
  document.documentElement.toggleAttribute("data-private", Boolean(event.private));
  const done = (command) => {
    send(command);
    send({ type: "hidePopup" });
  };
  const rows = event.rows.map((row) => {
    switch (row.kind ?? "item") {
      case "separator":
        return el("div", "sep");
      case "header":
        return el("div", "header", row.label);
      case "note":
        return el("div", "note", row.label);
      case "zoom": {
        const element = el("div", "zoom");
        element.append(el("span", "label", "Zoom"));
        const button = (icon, label, command, keepOpen = true) => {
          const control = el("button");
          control.innerHTML = icon;
          control.title = label;
          control.setAttribute("aria-label", label);
          onPress(control, () => (keepOpen ? send(command) : done(command)));
          return control;
        };
        const percent = el("button", "percent", `${row.percent}%`);
        percent.title = "Reset zoom";
        onPress(percent, () => send({ type: "zoomReset" }));
        element.append(
          button(ICONS.minus, "Zoom out", { type: "zoomOut" }),
          percent,
          button(ICONS.plus, "Zoom in", { type: "zoomIn" }),
          button(ICONS.expand, "Full screen", { type: "windowAction", action: "toggleFullscreen" }, false),
        );
        return element;
      }
      case "extension": {
        const element = el("div", "item ext-row");
        element.setAttribute("role", "menuitem");
        if (row.iconData) {
          const image = el("img");
          image.alt = "";
          image.src = row.iconData;
          element.append(image);
        } else {
          let hue = 0;
          for (const char of row.name) {
            hue = (hue * 31 + char.codePointAt(0)) % 360;
          }
          const letter = el("span", "letter", [...row.name.trim()][0]?.toUpperCase() ?? "?");
          letter.style.setProperty("--hue", String(hue));
          element.append(letter);
        }
        element.append(el("span", "label", row.name));
        const pin = el("button", "pin");
        pin.innerHTML = ICONS.pin;
        pin.title = row.pinned ? "Unpin from toolbar" : "Pin to toolbar";
        pin.setAttribute("aria-pressed", String(row.pinned));
        onPress(pin, () => send({ type: "setExtensionPinned", id: row.id, pinned: !row.pinned }));
        element.append(pin);
        onPress(element, () => {
          if (row.popupUrl) {
            done({ type: "openExtensionPopup", id: row.id, left: row.anchor.left, top: row.anchor.top });
          } else {
            done({ type: "openPage", page: "extensions" });
          }
        });
        return element;
      }
      default: {
        const element = el("div", "item");
        element.setAttribute("role", "menuitem");
        element.append(el("span", "label", row.label));
        if (row.shortcut) {
          element.append(el("span", "shortcut", row.shortcut));
        }
        onPress(element, () => done(row.command));
        return element;
      }
    }
  });
  const container = document.getElementById("items");
  container.className = "menu";
  container.setAttribute("role", "menu");
  container.replaceChildren(...rows);
}

// Titles and URLs come from websites, so they're only ever set as text.
window.onBrowserEvent = (event) => {
  if (event.type === "menu") {
    renderMenu(event);
    return;
  }
  if (event.type !== "items") {
    return;
  }
  document.getElementById("items").className = "";
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
