//! The page inspector: hover any text on a web page to see its font, size, weight and
//! colours, like the WhatFont and ColorZilla extensions.

/// Run in a web page's main frame. It adds a small floating card that follows the pointer,
/// outlines the element under it, and shows how it's styled. A click copies the details
/// and stops; Escape stops; running it again stops. It reads the page and changes nothing
/// in it, and it takes no input from outside: this text is all there is.
pub const INSPECT_SCRIPT: &str = r##"(function () {
  if (window.__stardustInspect) { window.__stardustInspect(); return; }
  var host = document.createElement("div");
  host.style.cssText = "all:initial;position:fixed;inset:0;z-index:2147483647;pointer-events:none;";
  var root = host.attachShadow({ mode: "closed" });
  var outline = document.createElement("div");
  outline.style.cssText = "position:fixed;border:2px solid #2f6fed;background:rgba(47,111,237,.12);pointer-events:none;display:none;box-sizing:border-box;";
  var card = document.createElement("div");
  card.style.cssText = "position:fixed;display:none;max-width:320px;padding:10px 12px;border-radius:8px;background:#1c1d21;color:#f3f4f6;font:12px/1.5 system-ui,sans-serif;box-shadow:0 4px 18px rgba(0,0,0,.35);pointer-events:none;white-space:normal;";
  root.append(outline, card);
  document.documentElement.append(host);

  function hex(color) {
    var m = String(color).match(/rgba?\(([^)]+)\)/);
    if (!m) return String(color);
    var p = m[1].split(/[ ,\/]+/).filter(Boolean).map(Number);
    if (p.length > 3 && p[3] === 0) return "transparent";
    return "#" + p.slice(0, 3).map(function (n) { return ("0" + Math.round(n).toString(16)).slice(-2); }).join("").toUpperCase();
  }
  function background(element) {
    for (var e = element; e; e = e.parentElement) {
      var c = getComputedStyle(e).backgroundColor;
      if (hex(c) !== "transparent") return hex(c);
    }
    return "#FFFFFF";
  }
  function describe(element) {
    var s = getComputedStyle(element);
    var stack = s.fontFamily.split(",").map(function (f) { return f.trim().replace(/^["']|["']$/g, ""); });
    return {
      family: stack[0], stack: stack.join(", "), size: s.fontSize, weight: s.fontWeight, style: s.fontStyle,
      line: s.lineHeight, spacing: s.letterSpacing, color: hex(s.color), background: background(element),
      tag: element.tagName.toLowerCase()
    };
  }
  function line(label, value) {
    var row = document.createElement("div");
    var a = document.createElement("span"); a.textContent = label + " "; a.style.opacity = ".6";
    var b = document.createElement("span"); b.textContent = value;
    row.append(a, b);
    return row;
  }
  function swatch(color) {
    var s = document.createElement("span");
    s.style.cssText = "display:inline-block;width:10px;height:10px;border-radius:2px;margin-right:5px;vertical-align:-1px;border:1px solid rgba(255,255,255,.4);background:" + color;
    return s;
  }
  var last = null;
  var summary = "";
  function onMove(event) {
    var element = document.elementFromPoint(event.clientX, event.clientY);
    if (!element || element === document.documentElement) { card.style.display = "none"; outline.style.display = "none"; return; }
    last = element;
    var d = describe(element);
    summary = d.family + ", " + d.size + ", weight " + d.weight + ", " + d.color + " on " + d.background + " (" + d.stack + ")";
    var box = element.getBoundingClientRect();
    outline.style.cssText += ";display:block;left:" + box.left + "px;top:" + box.top + "px;width:" + box.width + "px;height:" + box.height + "px;";
    card.replaceChildren();
    var title = document.createElement("div");
    title.textContent = d.family;
    title.style.cssText = "font-weight:600;font-size:14px;margin-bottom:4px;";
    var fg = line("Text", d.color); fg.prepend(swatch(d.color));
    var bg = line("Behind", d.background); bg.prepend(swatch(d.background));
    var hint = document.createElement("div");
    hint.textContent = "Click to copy. Esc to stop.";
    hint.style.cssText = "opacity:.55;margin-top:6px;";
    card.append(title, line("Size", d.size + "  ·  weight " + d.weight + (d.style !== "normal" ? "  ·  " + d.style : "")),
      line("Line", d.line + (d.spacing !== "normal" ? "  ·  spacing " + d.spacing : "")), fg, bg, line("Stack", d.stack), hint);
    card.style.display = "block";
    var x = event.clientX + 16, y = event.clientY + 16;
    var w = card.offsetWidth, h = card.offsetHeight;
    if (x + w > innerWidth - 8) x = event.clientX - w - 16;
    if (y + h > innerHeight - 8) y = event.clientY - h - 16;
    card.style.left = Math.max(8, x) + "px";
    card.style.top = Math.max(8, y) + "px";
  }
  function stop() {
    document.removeEventListener("mousemove", onMove, true);
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("keydown", onKey, true);
    host.remove();
    delete window.__stardustInspect;
  }
  function onClick(event) {
    event.preventDefault();
    event.stopPropagation();
    if (summary) {
      var field = document.createElement("textarea");
      field.value = summary;
      field.style.cssText = "position:fixed;opacity:0;";
      document.body.append(field);
      field.select();
      try { document.execCommand("copy"); } catch (error) {}
      field.remove();
    }
    stop();
  }
  function onKey(event) { if (event.key === "Escape") { event.preventDefault(); stop(); } }
  document.addEventListener("mousemove", onMove, true);
  document.addEventListener("click", onClick, true);
  document.addEventListener("keydown", onKey, true);
  window.__stardustInspect = stop;
})();"##;
