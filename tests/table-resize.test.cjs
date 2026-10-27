"use strict";

const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const scriptPath = join(__dirname, "../src/assets/ohimark.js");
const SITE = "https://example.test/docs/index.html";
const PAGE = "https://example.test/docs/guide.html";
const storageKey = (site = SITE, page = PAGE) => {
  const url = new URL(page);
  url.search = "";
  url.hash = "";
  return "ohimark-table-widths:" + JSON.stringify([site, url.href]);
};
const tableKey = (labels = ["Name", "Description"], occurrence = 0) =>
  JSON.stringify([labels.length, labels.map((label) => label.replace(/\s+/g, " ").trim()), occurrence]);

// The fixture models DOM events and table geometry. All resize, persistence and
// breakout decisions are made by the actual bundled script run below.
class Events {
  constructor() { this.listeners = new Map(); }
  addEventListener(type, callback) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type).add(callback);
  }
  removeEventListener(type, callback) { this.listeners.get(type)?.delete(callback); }
  invoke(event) {
    for (const callback of [...(this.listeners.get(event.type) || [])]) callback.call(this, event);
  }
  dispatchEvent(event) {
    prepareEvent(event, this);
    this.invoke(event);
    return !event.defaultPrevented;
  }
}

function prepareEvent(event, target) {
  event.target ??= target;
  event.defaultPrevented ??= false;
  event.preventDefault ??= () => { event.defaultPrevented = true; };
  event.stopPropagation ??= () => { event.stopped = true; };
}

function simpleMatches(element, selector) {
  let valid = true;
  selector = selector.replace(/:not\(([^)]+)\)/g, (_, excluded) => {
    if (simpleMatches(element, excluded)) valid = false;
    return "";
  });
  selector = selector.replace(/\[([\w-]+)(?:(\^?=)["']([^"']*)["'])?\]/g, (_, name, op, value) => {
    const actual = element.getAttribute(name);
    valid &&= actual !== null && (!op || (op === "=" ? actual === value : actual.startsWith(value)));
    return "";
  });
  if (!valid || selector.includes(":")) return false;
  const tag = selector.match(/^[\w-]+/);
  if (tag && element.tagName !== tag[0].toUpperCase()) return false;
  for (const match of selector.matchAll(/([.#])([\w-]+)/g)) {
    if (match[1] === "." ? !element.classList.contains(match[2]) : element.id !== match[2]) return false;
  }
  return true;
}

function matches(element, selector) {
  return selector.split(",").some((alternative) => {
    const parts = alternative.trim().replace(/\s*>\s*/g, ">").split(/(>|\s+)/).filter(Boolean);
    function at(node, index) {
      if (!node || !simpleMatches(node, parts[index])) return false;
      if (!index) return true;
      if (parts[index - 1] === ">") return at(node.parentElement, index - 2);
      for (let parent = node.parentElement; parent; parent = parent.parentElement) {
        if (at(parent, index - 2)) return true;
      }
      return false;
    }
    return at(element, parts.length - 1);
  });
}

function styleDeclaration() {
  const style = Object.create(null);
  const camel = (name) => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
  Object.defineProperties(style, {
    setProperty: { value(name, value) { style[camel(name)] = String(value); } },
    getPropertyValue: { value(name) { return style[camel(name)] || ""; } },
    removeProperty: { value(name) { const old = style[camel(name)] || ""; delete style[camel(name)]; return old; } },
  });
  return style;
}

class Element extends Events {
  constructor(tagName, ownerDocument) {
    super();
    this.tagName = tagName.toUpperCase();
    this.nodeName = this.tagName;
    this.nodeType = 1;
    this.ownerDocument = ownerDocument;
    this.attributes = new Map();
    this._children = [];
    this.parentNode = null;
    this.style = styleDeclaration();
    this.checked = false;
  }
  get parentElement() { return this.parentNode; }
  get childNodes() { return this._children; }
  get children() { return this._children.filter((node) => node.nodeType === 1); }
  get firstChild() { return this._children[0] || null; }
  get firstElementChild() { return this.children[0] || null; }
  get id() { return this.getAttribute("id") || ""; }
  set id(value) { this.setAttribute("id", value); }
  get className() { return this.getAttribute("class") || ""; }
  set className(value) { this.setAttribute("class", value); }
  get classList() {
    const element = this;
    return {
      contains(name) { return element.className.split(/\s+/).includes(name); },
      add(...names) { for (const name of names) if (!this.contains(name)) element.className = (element.className + " " + name).trim(); },
      remove(...names) { element.className = element.className.split(/\s+/).filter((name) => !names.includes(name)).join(" "); },
      toggle(name, force) {
        const on = force === undefined ? !this.contains(name) : force;
        if (on) this.add(name); else this.remove(name);
        return on;
      },
    };
  }
  get textContent() { return this.childNodes.map((child) => child.textContent).join(""); }
  set textContent(value) {
    for (const child of [...this.childNodes]) this.removeChild(child);
    if (String(value)) this.appendChild(new TextNode(String(value), this.ownerDocument));
  }
  get innerHTML() { return this._html || ""; }
  set innerHTML(value) { this.textContent = ""; this._html = String(value); }
  get href() { return new URL(this.getAttribute("href"), this.ownerDocument.URL).href; }
  get tabIndex() { return Number(this.getAttribute("tabindex") ?? -1); }
  set tabIndex(value) { this.setAttribute("tabindex", value); }
  get colSpan() { return Number(this.getAttribute("colspan") || 1); }
  set colSpan(value) { this.setAttribute("colspan", value); }
  get rowSpan() { return Number(this.getAttribute("rowspan") || 1); }
  set rowSpan(value) { this.setAttribute("rowspan", value); }
  get hidden() { return this.hasAttribute("hidden"); }
  set hidden(value) { if (value) this.setAttribute("hidden", ""); else this.removeAttribute("hidden"); }
  get inert() { return this.hasAttribute("inert"); }
  set inert(value) { if (value) this.setAttribute("inert", ""); else this.removeAttribute("inert"); }
  get tHead() { return this.children.find((child) => child.tagName === "THEAD") || null; }
  get rows() { return this.querySelectorAll("tr"); }
  get cells() { return this.children.filter((child) => child.tagName === "TD" || child.tagName === "TH"); }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  hasAttribute(name) { return this.attributes.has(name); }
  removeAttribute(name) { this.attributes.delete(name); }
  appendChild(child) { return this.insertBefore(child, null); }
  insertBefore(child, reference) {
    if (child.parentNode) child.parentNode.removeChild(child);
    const index = reference === null ? this._children.length : this._children.indexOf(reference);
    assert.ok(index >= 0, "insertBefore reference belongs to parent");
    this._children.splice(index, 0, child);
    child.parentNode = this;
    return child;
  }
  removeChild(child) {
    const index = this._children.indexOf(child);
    assert.ok(index >= 0, "removeChild node belongs to parent");
    this._children.splice(index, 1);
    child.parentNode = null;
    return child;
  }
  replaceChild(replacement, child) { this.insertBefore(replacement, child); return this.removeChild(child); }
  remove() { this.parentNode?.removeChild(this); }
  querySelectorAll(selector) {
    const found = [];
    for (const child of this.children) {
      if (matches(child, selector)) found.push(child);
      found.push(...child.querySelectorAll(selector));
    }
    return found;
  }
  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
  closest(selector) {
    for (let node = this; node; node = node.parentElement) if (matches(node, selector)) return node;
    return null;
  }
  contains(node) {
    for (; node; node = node.parentNode) if (node === this) return true;
    return false;
  }
  dispatchEvent(event) {
    prepareEvent(event, this);
    for (let node = this; node; node = event.bubbles === false ? null : node.parentNode) {
      node.invoke(event);
      if (event.stopped) return !event.defaultPrevented;
    }
    if (event.bubbles !== false) this.ownerDocument.defaultView.invoke(event);
    return !event.defaultPrevented;
  }
  focus() { this.ownerDocument.activeElement = this; }
  setPointerCapture(pointerId) { this.capturedPointer = pointerId; }
  hasPointerCapture(pointerId) { return this.capturedPointer === pointerId; }
  releasePointerCapture(pointerId) {
    if (this.capturedPointer !== pointerId) return;
    this.capturedPointer = null;
    this.dispatchEvent({ type: "lostpointercapture", pointerId });
  }
  getBoundingClientRect() { return this.ownerDocument.rect(this); }
  get clientWidth() { return this.getBoundingClientRect().width; }
  get offsetWidth() { return this.getBoundingClientRect().width; }
  get scrollWidth() {
    if (!this.classList.contains("ohm-table-scroll")) return this.clientWidth;
    const canvas = this.querySelector(".ohm-table-canvas");
    const table = canvas.querySelector("table");
    const minimum = table.spec.widths.length * 48 + table.spec.borders.reduce((sum, value) => sum + value, 0);
    const childExtents = this.children.map((child) => Number.parseFloat(child.style.width) || 0);
    return Math.max(Number.parseFloat(canvas.style.width) || this.clientWidth,
      Number.parseFloat(table.style.width) || minimum, ...childExtents);
  }
  get scrollLeft() {
    this._scrollLeft = Math.max(0, Math.min(this._scrollLeft || 0, Math.max(0, this.scrollWidth - this.clientWidth)));
    return this._scrollLeft;
  }
  set scrollLeft(value) { this._scrollLeft = Math.max(0, Number(value) || 0); }
}

class TextNode {
  constructor(text, ownerDocument) {
    this.nodeType = 3;
    this.nodeName = "#text";
    this.ownerDocument = ownerDocument;
    this.parentNode = null;
    this.textContent = text;
  }
  get parentElement() { return this.parentNode; }
  remove() { this.parentNode?.removeChild(this); }
}

class Storage {
  constructor(entries = []) { this.entries = new Map(entries); this.calls = []; this.block = new Set(); }
  getItem(key) {
    this.calls.push(["get", key]);
    if (this.block.has("get")) throw new Error("Storage read blocked");
    return this.entries.get(key) ?? null;
  }
  setItem(key, value) {
    this.calls.push(["set", key, value]);
    if (this.block.has("set")) throw new Error("Storage write blocked");
    this.entries.set(key, value);
  }
  removeItem(key) {
    this.calls.push(["remove", key]);
    if (this.block.has("remove")) throw new Error("Storage removal blocked");
    this.entries.delete(key);
  }
  get writes() { return this.calls.filter(([operation]) => operation !== "get"); }
}

function fixture({ specs = [{}], site = SITE, page = PAGE, storage = new Storage(),
  mainWidth = 1000, padding = 20, readingWidth = 600, optionalApis = true,
  panels = false, capture = true, blockedStorageGetter = false, beforeRun } = {}) {
  const document = new Element("#document");
  document.ownerDocument = document;
  document.URL = page;
  document.createElement = (tag) => new Element(tag, document);
  document.createTextNode = (text) => new TextNode(text, document);
  document.getElementById = (id) => document.querySelector("#" + id);
  const create = (tag, classes, parent, text = "") => {
    const element = document.createElement(tag);
    element.className = classes;
    element.textContent = text;
    if (parent) parent.appendChild(element);
    return element;
  };
  const events = new Events();
  const media = new Events();
  media.matches = false;
  const window = Object.assign(events, {
    sessionStorage: storage, innerWidth: 1400, location: { href: page },
    matchMedia: () => media,
  });
  document.defaultView = window;
  document.documentElement = create("html", "", document);
  document.body = create("body", "", document.documentElement);
  if (site !== null) create("a", "ohm-brand", document.body, "Site").setAttribute("href", site);
  let navState, outlineState;
  if (panels) {
    for (const [kind, panelId, buttonClass] of [["nav", "ohm-sidebar", "ohm-nav-button"], ["outline", "ohm-outline", "ohm-outline-button"]]) {
      const state = create("input", "ohm-" + kind + "-state", document.body);
      state.id = "ohm-" + kind + "-state";
      if (kind === "nav") navState = state; else outlineState = state;
      const panel = create("aside", panelId, document.body);
      panel.id = panelId;
      const label = create("label", buttonClass, panel);
      label.setAttribute("for", state.id);
    }
  }
  const main = create("main", "ohm-main", document.body);
  const article = create("article", "ohm-content", main);
  const tables = [];
  for (const spec of specs) {
    let parent = article;
    if (spec.container) {
      parent = create(spec.container, "", article);
      parent.indent = spec.indent ?? 40;
    }
    const table = create("table", "", parent);
    table.spec = { labels: ["Name", "Description"], widths: [200, 400], borders: [0, 0], ...spec };
    if (spec.colgroup) create("colgroup", "", table).appendChild(create("col", "", null));
    if (!spec.noHeader) {
      const head = create("thead", "", table);
      const row = create("tr", "", head);
      for (const label of table.spec.labels) create("th", "", row, label);
      if (spec.secondHeader) create("tr", "", head).appendChild(create("th", "", null, "Extra"));
    }
    if (!spec.headerOnly) {
      const body = create("tbody", "", table);
      const row = create("tr", "", body);
      for (const label of (spec.body || table.spec.labels)) create("td", "", row, label);
      if (spec.colSpan) row.cells[0].colSpan = spec.colSpan;
      if (spec.rowSpan) row.cells[0].rowSpan = spec.rowSpan;
      if (spec.nested) {
        const nested = create("table", "", row.cells[0]);
        const head = create("thead", "", nested);
        create("tr", "", head).appendChild(create("th", "", null, "Nested"));
        create("tbody", "", nested).appendChild(create("tr", "", null));
        nested.spec = { labels: ["Nested"], widths: [100], borders: [0, 0] };
      }
    }
    if (spec.footer) {
      const foot = create("tfoot", "authored-footer", table);
      for (const labels of spec.footer) {
        const row = create("tr", "authored-footer-row", foot);
        for (const label of labels) create("td", "", row, label);
      }
    }
    tables.push(table);
  }
  const frames = [], observers = [], readyCallbacks = [];
  const fonts = new Events();
  fonts.ready = { then(resolve) { readyCallbacks.push(resolve); } };
  const app = { document, window, storage, main, article, tables, observers, navState, outlineState };
  app.mainWidth = mainWidth;
  app.readingWidth = readingWidth;
  const px = (value, fallback = 0) => typeof value === "string" && value.endsWith("px") ? Number.parseFloat(value) : fallback;
  const box = (left, width, top = 0, height = 32) => ({ left, right: left + width, width, top, bottom: top + height, height, x: left, y: top });
  function mainBox() {
    const navGain = navState?.checked ? 160 : 0, outlineGain = outlineState?.checked ? 140 : 0;
    return box(100 - navGain, app.mainWidth + navGain + outlineGain);
  }
  function parentBox(element) { return document.rect(element.parentElement); }
  function chrome(table) { return table.spec.borders.reduce((sum, value) => sum + value, 0); }
  function tableWidth(table) {
    return px(table.style.width, Math.max(parentBox(table).width, table.spec.widths.length * 48 + chrome(table)));
  }
  function tracks(table) {
    const cols = table.children.find((child) => child.tagName === "COLGROUP")?.children || [];
    const specified = cols.map((col) => px(col.style.width, Number(col.getAttribute("width"))));
    if (specified.length === table.spec.widths.length && specified.every((value) => value > 0)) {
      const sum = specified.reduce((total, value) => total + value, 0);
      const total = px(table.style.width, sum + chrome(table)) - chrome(table);
      return specified.map((value) => value * total / sum);
    }
    const available = Math.max(table.spec.widths.length * 48, parentBox(table).width - chrome(table));
    const weights = [...table.spec.widths];
    const result = new Array(weights.length);
    let remaining = available, pending = weights.map((_, index) => index);
    while (pending.length) {
      const sum = pending.reduce((total, index) => total + weights[index], 0);
      const narrow = pending.filter((index) => remaining * weights[index] / sum < 48);
      if (!narrow.length) {
        for (const index of pending) result[index] = remaining * weights[index] / sum;
        break;
      }
      for (const index of narrow) { result[index] = 48; remaining -= 48; }
      pending = pending.filter((index) => !narrow.includes(index));
    }
    return result;
  }
  document.rect = (element) => {
    if (!element || element === document || element === document.body || element === document.documentElement) return box(0, 1400);
    if (element === main) return mainBox();
    if (element === article) {
      const mainRect = mainBox();
      const width = Math.min(app.readingWidth, mainRect.width - 2 * padding);
      return box(mainRect.left + (mainRect.width - width) / 2, width);
    }
    if (element.indent !== undefined) { const parent = parentBox(element); return box(parent.left + element.indent, parent.width - element.indent); }
    if (element.classList.contains("ohm-table-frame")) {
      const parent = parentBox(element);
      return box(parent.left + px(element.style.marginInlineStart, px(element.style.marginLeft)), px(element.style.width, parent.width));
    }
    if (element.classList.contains("ohm-table-scroll")) return parentBox(element);
    if (element.classList.contains("ohm-table-canvas")) {
      const parent = parentBox(element);
      return box(parent.left - element.parentElement.scrollLeft, px(element.style.width, parent.width));
    }
    if (element.tagName === "TABLE") {
      const parent = parentBox(element);
      const rows = element.querySelectorAll("tr").filter((row) => row.closest("table") === element);
      return box(parent.left, tableWidth(element), 0, rows.length * 32);
    }
    if (element.tagName === "TH" || element.tagName === "TD") {
      const table = element.closest("table");
      const rect = document.rect(table), widths = tracks(table);
      const row = element.parentElement, cells = row.cells;
      const index = cells.slice(0, cells.indexOf(element)).reduce((offset, cell) => offset + cell.colSpan, 0);
      const rows = table.querySelectorAll("tr").filter((candidate) => candidate.closest("table") === table);
      const top = rows.indexOf(row) * 32;
      return box(rect.left + table.spec.borders[0] + widths.slice(0, index).reduce((sum, value) => sum + value, 0),
        widths.slice(index, index + element.colSpan).reduce((sum, value) => sum + value, 0), top, 32);
    }
    if (element.classList.contains("ohm-table-column-handle")) {
      const cell = parentBox(element);
      const final = element.parentElement === element.parentElement.parentElement.cells.at(-1);
      return box(cell.right - (final ? 14 : 6), 12, cell.top);
    }
    if (element.classList.contains("ohm-table-actions")) {
      const cell = parentBox(element);
      return box(cell.left + 2, cell.width - 4, cell.top + 2, cell.height - 4);
    }
    if (element.classList.contains("ohm-table-reset")) {
      const actions = parentBox(element);
      return box(actions.right - 28, 28, actions.top, 28);
    }
    return parentBox(element);
  };
  window.getComputedStyle = (element) => ({
    paddingLeft: element === main ? padding + "px" : "0px",
    paddingRight: element === main ? padding + "px" : "0px",
    borderLeftWidth: element.tagName === "TABLE" ? element.spec.borders[0] + "px" : "0px",
    borderRightWidth: element.tagName === "TABLE" ? element.spec.borders[1] + "px" : "0px",
    boxSizing: "border-box", display: "block", direction: "ltr",
  });
  if (!capture) {
    document.createElement = (tag) => {
      const element = new Element(tag, document);
      element.setPointerCapture = element.releasePointerCapture = element.hasPointerCapture = undefined;
      return element;
    };
  }
  if (optionalApis) {
    window.requestAnimationFrame = (callback) => frames.push(callback);
    window.ResizeObserver = class {
      constructor(callback) { this.callback = callback; this.targets = []; observers.push(this); }
      observe(target) { this.targets.push(target); }
    };
    document.fonts = fonts;
  }
  if (blockedStorageGetter) Object.defineProperty(window, "sessionStorage", { get() { throw new Error("Storage inaccessible"); } });
  app.flush = () => {
    for (let count = 0; frames.length; count++) {
      assert.ok(count < 100, "layout work settles without an animation frame loop");
      frames.shift()();
    }
  };
  app.widths = (index = 0) => tracks(tables[index]);
  app.frame = (index = 0) => tables[index].closest(".ohm-table-frame");
  app.scroll = (index = 0) => app.frame(index)?.querySelector(".ohm-table-scroll");
  app.column = (index = 0, table = 0) => tables[table].querySelectorAll(".ohm-table-column-handle")[index];
  app.toggleButton = (index = 0, table = 0) => tables[table].querySelectorAll(".ohm-table-column-toggle")[index];
  app.toggle = (index = 0, table = 0) => { app.toggleButton(index, table).dispatchEvent({ type: "click" }); app.flush(); };
  app.reset = (table = 0) => { app.frame(table).querySelector(".ohm-table-reset").dispatchEvent({ type: "click" }); app.flush(); };
  app.key = (handle, key, shiftKey = false) => { const event = { type: "keydown", key, shiftKey }; handle.dispatchEvent(event); app.flush(); return event; };
  app.down = (handle, clientX, options = {}) => {
    const event = { type: "pointerdown", clientX, pointerId: 1, button: 0, isPrimary: true, ...options };
    handle.dispatchEvent(event); app.flush(); return event;
  };
  app.move = (clientX, options = {}) => { window.dispatchEvent({ type: "pointermove", clientX, pointerId: 1, ...options }); app.flush(); };
  app.up = (clientX, options = {}) => { window.dispatchEvent({ type: "pointerup", clientX, pointerId: 1, ...options }); app.flush(); };
  app.cancel = () => { window.dispatchEvent({ type: "pointercancel", pointerId: 1 }); app.flush(); };
  app.resize = (width = app.mainWidth) => {
    app.mainWidth = width; window.dispatchEvent({ type: "resize" });
    for (const observer of observers) observer.callback();
    app.flush();
  };
  app.fontsReady = () => { readyCallbacks.forEach((callback) => callback()); app.flush(); };
  app.fontsLoaded = () => { fonts.dispatchEvent({ type: "loadingdone" }); app.flush(); };
  app.panel = (kind) => { document.querySelector(".ohm-" + kind + "-button").dispatchEvent({ type: "click" }); app.flush(); };
  app.saved = () => JSON.parse(storage.entries.get(storageKey(site, page)) || "{}");
  if (beforeRun) beforeRun(app);
  vm.runInNewContext(readFileSync(scriptPath, "utf8"), { document, window, navigator: {}, URL, setTimeout }, { filename: "ohimark.js" });
  app.flush();
  return app;
}

function close(actual, expected, message = "values agree") {
  assert.ok(Math.abs(actual - expected) < 0.00001, `${message}: ${actual} != ${expected}`);
}
function closeWidths(actual, expected) {
  assert.equal(actual.length, expected.length);
  actual.forEach((value, index) => close(value, expected[index], "column " + index));
}
function boundary(app, index = 0, table = 0) {
  return app.tables[table].querySelectorAll("thead th")[index].getBoundingClientRect().right;
}
function dragBy(app, handle, delta, x = handle.getBoundingClientRect().right) {
  app.down(handle, x); app.move(x + delta); app.up(x + delta);
}
function hidden(node) {
  for (; node; node = node.parentElement) {
    if (node.nodeType === 1 && (node.hidden || node.inert || node.style.display === "none")) return true;
  }
  return false;
}
function authoredCells(app, index, tableIndex = 0) {
  const table = app.tables[tableIndex];
  return table.querySelectorAll("tr").filter((row) => row.closest("table") === table
    && !row.classList.contains("ohm-table-controls-row")).map((row) => row.cells[index]);
}

test("only simple rectangular tables receive working controls", () => {
  const app = fixture({ specs: [{}, { colSpan: 2 }, { rowSpan: 2 }, { colgroup: true },
    { noHeader: true }, { nested: true }, { secondHeader: true }, { body: ["Missing cell"] }] });
  assert.equal(app.document.querySelectorAll(".ohm-table-frame").length, 1);
  assert.equal(app.tables[0].querySelectorAll(".ohm-table-column-handle").length, 2);
  assert.equal(app.frame().querySelectorAll('[role="separator"]').length, 2, "only column grips resize the table");
  assert.equal(app.frame().querySelector(".ohm-table-reset").disabled, true);
  assert.equal(app.tables[0].querySelector("colgroup"), null, "automatic layout has no generated column widths");
  assert.deepEqual(app.storage.writes, []);
  const empty = fixture({ specs: [], blockedStorageGetter: true });
  assert.equal(empty.document.querySelector(".ohm-table-frame"), null);
});

test("the reset icon is the only footer control and preserves authored footer rows", () => {
  for (const spec of [{}, { headerOnly: true }, { footer: [["Total", "2"], ["Source", "Author"]] },
    { labels: ["Only"], widths: [48], headerOnly: true }]) {
    let authoredFooter, authoredRows;
    const storage = new Storage(spec.labels?.length === 1
      ? [[storageKey(), JSON.stringify({ [tableKey(spec.labels)]: [48] })]] : []);
    const app = fixture({ specs: [spec], storage, beforeRun({ tables }) {
      authoredFooter = tables[0].children.find((child) => child.tagName === "TFOOT");
      authoredRows = authoredFooter ? [...authoredFooter.children] : [];
    } });
    const table = app.tables[0], footers = table.children.filter((child) => child.tagName === "TFOOT");
    assert.equal(footers.length, 1);
    const footer = footers[0], controlsRows = footer.querySelectorAll(".ohm-table-controls-row");
    assert.equal(controlsRows.length, 1);
    if (authoredFooter) assert.equal(footer, authoredFooter, "existing footer is reused");
    assert.deepEqual(footer.children.slice(0, authoredRows.length), authoredRows, "authored rows retain their identity and order");
    if (spec.footer) assert.deepEqual(authoredRows.map((row) => row.cells.map((cell) => cell.textContent)), spec.footer);
    const cell = controlsRows[0].querySelector(".ohm-table-controls-cell");
    assert.equal(cell.colSpan, table.spec.labels.length);
    close(cell.getBoundingClientRect().width, app.widths().reduce((total, width) => total + width, 0), "footer cell spans every column");
    const actions = cell.querySelector(".ohm-table-actions"), reset = actions.querySelector(".ohm-table-reset");
    assert.equal(actions.closest("table"), table);
    assert.equal(app.frame().querySelectorAll(".ohm-table-actions").length, 1);
    assert.deepEqual(app.frame().children.map((child) => child.className), ["ohm-table-scroll"], "no exterior control area remains");
    assert.equal(reset.tagName, "BUTTON");
    assert.equal(reset.classList.contains("ohm-icon-button"), true);
    assert.equal(reset.getAttribute("aria-label"), "Reset widths");
    assert.equal(reset.getAttribute("title"), "Reset widths");
    assert.match(reset.innerHTML, /<svg\b[^>]*class="ohm-icon"/);
    assert.equal(reset.innerHTML.replace(/<[^>]*>/g, "").trim(), "", "reset has no visible text label");
    assert.deepEqual(actions.children, [reset], "footer contains only Reset widths");
    assert.equal(app.frame().querySelectorAll('[role="separator"]').length, table.spec.labels.length);
    app.key(app.column(table.spec.labels.length - 1), "ArrowRight");
    assert.equal(reset.disabled, false);
    app.reset();
    assert.equal(reset.disabled, true);
    assert.deepEqual(footer.children.slice(0, authoredRows.length), authoredRows);
  }
});

test("column keyboard resizing keeps other columns fixed and updates accessible width", () => {
  const app = fixture();
  const handle = app.column();
  assert.equal(handle.getAttribute("role"), "separator");
  assert.equal(handle.getAttribute("aria-orientation"), "vertical");
  assert.equal(handle.tabIndex, 0);
  assert.match(handle.getAttribute("aria-label"), /Name/);
  assert.equal(Number(handle.getAttribute("aria-valuemin")), 48);
  assert.equal(app.key(handle, "ArrowRight").defaultPrevented, true);
  closeWidths(app.widths(), [201, 400]);
  close(app.tables[0].getBoundingClientRect().width, 601);
  assert.equal(Number(handle.getAttribute("aria-valuenow")), 201);
  assert.match(handle.getAttribute("aria-valuetext"), /201/);
  app.key(handle, "ArrowLeft", true);
  closeWidths(app.widths(), [191, 400]);
  assert.equal(app.frame().querySelector(".ohm-table-reset").disabled, false);
  assert.equal(app.tables[0].classList.contains("ohm-table--fixed"), true);
  closeWidths(app.saved()[tableKey()], [191, 400]);
});

test("the final column grip adjusts total width by 1px or 10px and clamps at 48px without changing earlier columns", () => {
  const app = fixture();
  const handle = app.column(1);
  app.key(handle, "ArrowRight");
  closeWidths(app.widths(), [200, 401]);
  close(app.tables[0].getBoundingClientRect().width, 601);
  app.key(handle, "ArrowRight", true);
  closeWidths(app.widths(), [200, 411]);
  close(app.tables[0].getBoundingClientRect().width, 611);
  app.key(handle, "ArrowLeft", true);
  closeWidths(app.widths(), [200, 401]);
  assert.equal(handle.tabIndex, 0);
  assert.equal(handle.getAttribute("role"), "separator");
  assert.match(handle.getAttribute("aria-label"), /Description/);
  dragBy(app, handle, -2000);
  closeWidths(app.widths(), [200, 48]);
  close(app.tables[0].getBoundingClientRect().width, 248);
  app.key(handle, "ArrowLeft", true);
  closeWidths(app.widths(), [200, 48]);
});

test("initially narrow columns honor the minimum before the first interaction", () => {
  const app = fixture({ specs: [{ widths: [8, 592] }] });
  closeWidths(app.widths(), [48, 552]);
  app.key(app.column(1), "ArrowRight");
  closeWidths(app.widths(), [48, 553]);
  assert.deepEqual(app.storage.writes.map(([operation]) => operation), ["set"]);
});

test("automatic overflowing tables keep the final column grip at their real edge across reset and viewport changes", () => {
  const labels = Array.from({ length: 15 }, (_, index) => "Column " + index);
  const app = fixture({ specs: [{ labels, widths: labels.map(() => 1) }] });
  function verifyAutomatic(expectedTableWidth, expectedFrameWidth) {
    assert.equal(app.tables[0].classList.contains("ohm-table--fixed"), false);
    assert.equal(app.tables[0].querySelector("colgroup"), null);
    close(app.tables[0].getBoundingClientRect().width, expectedTableWidth);
    close(app.frame().getBoundingClientRect().width, expectedFrameWidth);
    app.scroll().scrollLeft = 10000;
    const grip = app.column(labels.length - 1).getBoundingClientRect(), table = app.tables[0].getBoundingClientRect();
    assert.ok(grip.left >= table.left && grip.right <= table.right && grip.top >= table.top && grip.bottom <= table.bottom,
      "final column grip stays inside the table header");
    assert.ok(grip.right > table.right - 48, "final column grip follows the table's actual trailing edge");
  }
  verifyAutomatic(720, 600);
  assert.equal(app.scroll().scrollLeft, 120);
  assert.deepEqual(app.storage.writes, []);
  const nativeEdge = app.tables[0].getBoundingClientRect().right;
  app.down(app.column(labels.length - 1), nativeEdge);
  app.up(nativeEdge);
  verifyAutomatic(720, 600);
  app.down(app.column(labels.length - 1), nativeEdge);
  app.move(nativeEdge + 10);
  app.move(nativeEdge);
  app.up(nativeEdge);
  verifyAutomatic(720, 600);
  assert.deepEqual(app.storage.writes, [], "unchanged and returned-to-origin automatic drags do not create preferences");
  app.key(app.column(), "ArrowRight");
  app.reset();
  assert.equal(app.scroll().scrollLeft, 0, "reset clears horizontal scroll");
  verifyAutomatic(720, 600);
  app.readingWidth = 800;
  app.resize(1200);
  verifyAutomatic(800, 800);
  app.resize(400);
  verifyAutomatic(720, 360);
  app.readingWidth = 600;
  app.resize(1000);
  verifyAutomatic(720, 600);
});

test("a dragged column boundary follows the pointer across reading and main-pane thresholds", () => {
  const app = fixture();
  const handle = app.column();
  const start = boundary(app);
  app.down(handle, start);
  app.move(start + 60);
  close(boundary(app), start + 60, "boundary follows pointer while wrapper grows and centers");
  closeWidths(app.widths(), [320, 400]);
  close(app.frame().getBoundingClientRect().width, 720);
  app.move(start + 260);
  close(boundary(app), start + 260, "boundary follows pointer beyond main-pane cap");
  closeWidths(app.widths(), [640, 400]);
  close(app.frame().getBoundingClientRect().width, 960);
  close(app.frame().getBoundingClientRect().left, 120);
  assert.equal(app.storage.writes.length, 0, "pointer movement does not persist until commit");
  app.up(start + 260);
  closeWidths(app.saved()[tableKey()], [640, 400]);
  assert.equal(app.document.documentElement.classList.contains("ohm-table-resizing"), false);
});

test("dragging the final column tracks the outer right edge while earlier columns stay fixed", () => {
  const app = fixture();
  const start = app.tables[0].getBoundingClientRect().right;
  app.down(app.column(1), start);
  app.move(start + 60);
  close(app.tables[0].getBoundingClientRect().right, start + 60);
  closeWidths(app.widths(), [200, 520]);
  app.move(start + 250);
  close(app.tables[0].getBoundingClientRect().right, start + 250);
  closeWidths(app.widths(), [200, 830]);
  app.up(start + 250);
});

test("a drag uses horizontal scroll changes as well as pointer coordinates", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [600, 600] })]]);
  const app = fixture({ storage });
  const scroll = app.scroll();
  scroll.scrollLeft = 100;
  const start = boundary(app);
  app.down(app.column(), start);
  scroll.scrollLeft = 140;
  scroll.dispatchEvent({ type: "scroll" });
  app.move(start + 20);
  close(boundary(app), start + 20);
  closeWidths(app.widths(), [660, 600]);
  app.up(start + 20);
});

test("shrinking a scrolled table accounts for scrollLeft clamping as its scroll range contracts", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [600, 600] })]]);
  const app = fixture({ storage });
  const scroll = app.scroll();
  scroll.scrollLeft = 240;
  const start = app.tables[0].getBoundingClientRect().right;
  app.down(app.column(1), start);
  app.move(start - 100);
  close(app.tables[0].getBoundingClientRect().right, start - 100, "outer boundary follows pointer after scroll range shrinks");
  app.up(start - 100);
});

test("small final-column shrinks at the scroll limit leave earlier widths fixed and repeated coordinates do not ratchet", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [600, 600] })]]);
  const app = fixture({ storage, mainWidth: 540 });
  const scroll = app.scroll();
  close(scroll.clientWidth, 500);
  scroll.scrollLeft = 700;
  const handle = app.column(1), start = boundary(app, 1);
  app.down(handle, start);
  app.move(start - 10);
  const expected = [600, 590];
  closeWidths(app.widths(), expected);
  close(app.tables[0].getBoundingClientRect().width, 1190);
  close(scroll.scrollLeft, 700, "drag preserves the initial scroll range");
  const moved = app.widths();
  for (let count = 0; count < 3; count++) {
    app.move(start - 10);
    closeWidths(app.widths(), moved);
  }
  assert.equal(storage.writes.length, 0);
  app.up(start - 10);
  closeWidths(app.widths(), moved);
  closeWidths(app.saved()[tableKey()], expected);
  assert.equal(storage.writes.length, 1);
  close(scroll.scrollWidth, 1190, "temporary drag extent is removed after commit");
  close(scroll.scrollLeft, 690, "scroll position is clamped only after completion");
  assert.equal(app.frame().querySelector(".ohm-table-drag-spacer"), null);

  const nextStart = boundary(app, 1);
  app.down(handle, nextStart);
  app.move(nextStart - 10);
  app.key(handle, "Escape");
  closeWidths(app.widths(), expected);
  close(scroll.scrollLeft, 690);
  assert.equal(app.frame().querySelector(".ohm-table-drag-spacer"), null, "cancel removes temporary extent");
  assert.equal(storage.writes.length, 1, "cancel preserves committed widths without another write");
});

test("newly created scroll range stays stable when a table grows and shrinks in one drag", () => {
  const app = fixture();
  const scroll = app.scroll();
  const start = app.tables[0].getBoundingClientRect().right;
  app.down(app.column(1), start);
  app.move(start + 300);
  close(app.tables[0].getBoundingClientRect().width, 1080);
  scroll.scrollLeft = 120;
  const target = app.tables[0].getBoundingClientRect().right - 10;
  app.move(target);
  close(app.tables[0].getBoundingClientRect().width, 1070);
  close(scroll.scrollLeft, 120);
  const widths = app.widths();
  app.move(target);
  app.up(target);
  closeWidths(app.widths(), widths);
  closeWidths(widths, [200, 870]);
  close(scroll.scrollWidth, 1070);
  close(scroll.scrollLeft, 110);
});

test("reset restores automatic sizing and removes only that table's preference", () => {
  const app = fixture({ specs: [{}, {}] });
  app.key(app.column(0, 0), "ArrowRight");
  app.key(app.column(1, 1), "ArrowRight", true);
  assert.equal(Object.keys(app.saved()).length, 2);
  app.reset(0);
  closeWidths(app.widths(0), [200, 400]);
  assert.equal(app.tables[0].classList.contains("ohm-table--fixed"), false);
  assert.equal(app.tables[0].querySelector("colgroup"), null);
  assert.equal(app.saved()[tableKey()], undefined);
  closeWidths(app.saved()[tableKey(undefined, 1)], [200, 410]);
  assert.equal(app.frame(0).querySelector(".ohm-table-reset").disabled, true);
  app.reset(1);
  assert.equal(app.storage.entries.has(storageKey()), false);
});

test("Escape, pointercancel and unexpected lost capture restore automatic, fixed or scrolled layout without saving", () => {
  for (const cancel of ["Escape", "pointercancel", "lostpointercapture"]) {
    for (const state of ["automatic", "fixed", "scrolled"]) {
      const scrolled = state === "scrolled", fixed = state !== "automatic";
      const app = scrolled
        ? fixture({ mainWidth: 540, storage: new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [600, 600] })]]) })
        : fixture();
      if (state === "fixed") app.key(app.column(1), "ArrowRight", true);
      if (scrolled) app.scroll().scrollLeft = 700;
      const widths = app.widths(), writes = app.storage.writes.length;
      const handle = scrolled ? app.column(1) : app.column();
      const start = scrolled ? app.tables[0].getBoundingClientRect().right : boundary(app);
      app.down(handle, start);
      app.move(start + (scrolled ? -10 : 40));
      assert.notDeepEqual(app.widths(), widths);
      if (cancel === "Escape") app.key(handle, "Escape");
      else if (cancel === "pointercancel") app.cancel();
      else handle.dispatchEvent({ type: "lostpointercapture", pointerId: 1 });
      app.flush();
      closeWidths(app.widths(), widths);
      assert.equal(app.storage.writes.length, writes, cancel + " does not persist a transient width");
      assert.equal(app.tables[0].classList.contains("ohm-table--fixed"), fixed);
      assert.equal(app.document.documentElement.classList.contains("ohm-table-resizing"), false);
      assert.equal(app.frame().querySelector(".ohm-table-drag-spacer"), null);
      app.move(start + 100);
      closeWidths(app.widths(), widths);
      if (scrolled) {
        close(app.scroll().scrollLeft, 700, cancel + " restores the original scroll position");
        app.key(app.column(1), "ArrowLeft", true);
        close(app.scroll().scrollWidth, 1190, "later keyboard resize has no stale drag extent");
        close(app.scroll().scrollLeft, 690);
      }
    }
  }
});

test("unchanged drags, secondary buttons, nonprimary pointers and unrelated pointer IDs do not save", () => {
  const app = fixture();
  const handle = app.column(), start = boundary(app);
  app.down(handle, start); app.up(start);
  assert.equal(app.tables[0].classList.contains("ohm-table--fixed"), false);
  for (const options of [{ button: 2 }, { isPrimary: false }]) {
    app.down(handle, start, options); app.move(start + 40); app.up(start + 40);
    closeWidths(app.widths(), [200, 400]);
  }
  app.down(handle, start);
  app.move(start + 40, { pointerId: 2 });
  app.up(start + 40, { pointerId: 2 });
  closeWidths(app.widths(), [200, 400]);
  app.cancel();
  assert.deepEqual(app.storage.writes, []);
});

test("pointer fallback remains functional without capture or optional layout APIs", () => {
  const app = fixture({ capture: false, optionalApis: false });
  dragBy(app, app.column(), 20, boundary(app));
  closeWidths(app.widths(), [240, 400]);
  assert.equal(app.storage.writes.length, 1);
});

test("pixel widths restore across reloads and canonical URL variants without redundant writes", () => {
  const app = fixture({ specs: [{ labels: ["  Name \n", " Description  "] }] });
  app.key(app.column(), "ArrowRight", true);
  const writes = app.storage.writes.length;
  const restored = fixture({ page: PAGE + "?view=other#section", storage: app.storage, specs: [{ body: ["Changed", "Body"] }] });
  closeWidths(restored.widths(), [210, 400]);
  assert.equal(app.storage.writes.length, writes);
  assert.equal(restored.tables[0].classList.contains("ohm-table--fixed"), true);
  assert.equal(restored.frame().querySelector(".ohm-table-reset").disabled, false);
  const fresh = fixture();
  closeWidths(fresh.widths(), [200, 400]);
  assert.deepEqual(fresh.storage.writes, []);
});

test("sites, pages and identical-header occurrences have separate preferences", () => {
  const storage = new Storage();
  const first = fixture({ specs: [{}, {}], storage });
  first.key(first.column(0, 0), "ArrowRight");
  first.key(first.column(1, 1), "ArrowRight", true);
  const otherPage = fixture({ page: "https://example.test/docs/other.html", storage });
  closeWidths(otherPage.widths(), [200, 400]);
  otherPage.key(otherPage.column(), "ArrowRight", true);
  const otherSite = fixture({ site: "https://example.test/elsewhere/index.html", storage });
  closeWidths(otherSite.widths(), [200, 400]);
  otherSite.key(otherSite.column(1), "ArrowLeft");
  const restored = fixture({ specs: [{}, {}], storage });
  closeWidths(restored.widths(0), [201, 400]);
  closeWidths(restored.widths(1), [200, 410]);
  restored.reset(0);
  assert.ok(storage.entries.has(storageKey("https://example.test/elsewhere/index.html")));
  assert.ok(storage.entries.has(storageKey(SITE, "https://example.test/docs/other.html")));
});

test("header changes and unrelated table insertions do not apply another table's widths", () => {
  const app = fixture();
  app.key(app.column(), "ArrowRight", true);
  const changed = fixture({ storage: app.storage, specs: [{ labels: ["Renamed", "Description"] }] });
  closeWidths(changed.widths(), [200, 400]);
  const inserted = fixture({ storage: new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [210, 400] })]]),
    specs: [{ labels: ["Different", "Table"] }, {}] });
  closeWidths(inserted.widths(0), [200, 400]);
  closeWidths(inserted.widths(1), [210, 400]);
});

test("fractional column widths and table borders survive repeated save/restore without drift", () => {
  const spec = { widths: [199.25, 399.25], borders: [0.75, 0.75] };
  let app = fixture({ specs: [spec] });
  app.key(app.column(), "ArrowRight");
  const expected = [200.25, 399.25];
  closeWidths(app.widths(), expected);
  close(app.tables[0].getBoundingClientRect().width, 601);
  for (let index = 0; index < 8; index++) {
    const writes = app.storage.writes.length;
    app = fixture({ specs: [spec], storage: app.storage });
    closeWidths(app.widths(), expected);
    close(app.tables[0].getBoundingClientRect().width, 601);
    assert.equal(app.storage.writes.length, writes);
  }
});

test("malformed or incompatible storage is discarded without disabling controls", () => {
  const invalid = ["{broken", "null", "[]", '"bad"', "17",
    JSON.stringify({ [tableKey()]: [10, 590] }), JSON.stringify({ [tableKey()]: [200] }),
    JSON.stringify({ [tableKey()]: [200, null] }), JSON.stringify({ [tableKey()]: [200, "400"] }),
    '{' + JSON.stringify(tableKey()) + ':[1e999,400]}',
    JSON.stringify({ [tableKey()]: [Number.MAX_VALUE, Number.MAX_VALUE] })];
  for (const stored of invalid) {
    const storage = new Storage([[storageKey(), stored]]);
    const app = fixture({ storage });
    closeWidths(app.widths(), [200, 400]);
    assert.equal(storage.entries.has(storageKey()), false, stored);
    app.key(app.column(), "ArrowRight");
    closeWidths(app.widths(), [201, 400]);
  }
});

test("cleaning an incompatible entry preserves other tables' valid widths", () => {
  const values = { [tableKey()]: [210, 400], [tableKey(undefined, 1)]: [47, 400], obsolete: [200, 400] };
  const storage = new Storage([[storageKey(), JSON.stringify(values)]]);
  const app = fixture({ specs: [{}, {}], storage });
  closeWidths(app.widths(0), [210, 400]);
  closeWidths(app.widths(1), [200, 400]);
  assert.deepEqual(app.saved(), { [tableKey()]: [210, 400] });
  assert.equal(storage.writes.length, 1);
});

test("storage read, write, removal and getter failures never disable current-document resizing", () => {
  for (const failure of ["get", "set", "remove", "getter"]) {
    const storage = new Storage();
    if (failure !== "getter") storage.block.add(failure);
    const app = fixture({ storage, blockedStorageGetter: failure === "getter" });
    app.key(app.column(), "ArrowRight");
    closeWidths(app.widths(), [201, 400]);
    app.reset();
    closeWidths(app.widths(), [200, 400]);
  }
  const noBrand = fixture({ site: null });
  noBrand.key(noBrand.column(), "ArrowRight");
  closeWidths(noBrand.widths(), [201, 400]);
  assert.deepEqual(noBrand.storage.calls, []);
});

test("wrapper geometry responds to viewport and font changes while saved pixel widths stay fixed", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [600, 600] })]]);
  const app = fixture({ storage });
  close(app.frame().getBoundingClientRect().width, 960);
  app.resize(500);
  close(app.frame().getBoundingClientRect().width, 460);
  closeWidths(app.widths(), [600, 600]);
  assert.ok(app.scroll().scrollWidth > app.scroll().clientWidth);
  app.mainWidth = 800;
  app.fontsReady();
  close(app.frame().getBoundingClientRect().width, 760);
  app.mainWidth = 900;
  app.fontsLoaded();
  close(app.frame().getBoundingClientRect().width, 860);
  closeWidths(app.widths(), [600, 600]);
  assert.deepEqual(storage.writes, []);
});

test("nested list and blockquote tables resize within their own scrolling boundary", () => {
  for (const container of ["li", "blockquote"]) {
    const app = fixture({ specs: [{ container }] });
    const parent = app.frame().parentElement.getBoundingClientRect();
    dragBy(app, app.column(), 700, boundary(app));
    close(app.frame().getBoundingClientRect().left, parent.left);
    close(app.frame().getBoundingClientRect().width, parent.width);
    assert.ok(app.scroll().scrollWidth > app.scroll().clientWidth);
  }
});

test("programmatic navigation and outline toggles refresh breakout bounds without ResizeObserver", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [800, 800] })]]);
  const app = fixture({ storage, panels: true, optionalApis: false });
  close(app.frame().getBoundingClientRect().width, 960);
  app.panel("nav");
  assert.equal(app.navState.checked, true);
  close(app.frame().getBoundingClientRect().width, 1120);
  app.panel("outline");
  assert.equal(app.outlineState.checked, true);
  close(app.frame().getBoundingClientRect().width, 1260);
  closeWidths(app.widths(), [800, 800]);
  app.panel("nav");
  close(app.frame().getBoundingClientRect().width, 1100);
});

test("collapsing a column creates a 32px strip and gives its space to the remaining column", () => {
  const app = fixture();
  const button = app.toggleButton();
  assert.equal(button.tagName, "BUTTON");
  assert.equal(button.getAttribute("aria-expanded"), "true");
  assert.equal(button.getAttribute("aria-label"), "Collapse column Name");
  assert.equal(button.getAttribute("title"), "Collapse column Name");
  assert.deepEqual(app.storage.writes, [], "adding collapse buttons does not save a layout");
  app.toggle();
  closeWidths(app.widths(), [32, 568]);
  close(app.tables[0].getBoundingClientRect().width, 600);
  assert.equal(button.getAttribute("aria-expanded"), "false");
  assert.equal(button.getAttribute("aria-label"), "Expand column Name");
  assert.equal(button.getAttribute("title"), "Expand column Name");
  assert.equal(button.disabled, false);
  for (const cell of authoredCells(app, 0)) {
    assert.equal(cell.classList.contains("ohm-table-column-collapsed"), true);
    assert.equal(cell.querySelector(".ohm-table-cell-content").hidden, true);
  }
  assert.equal(hidden(button), false, "restore button remains available inside collapsed header");
  app.toggle();
  closeWidths(app.widths(), [200, 400]);
  assert.equal(button.getAttribute("aria-expanded"), "true");
  for (const cell of authoredCells(app, 0)) {
    assert.equal(cell.classList.contains("ohm-table-column-collapsed"), false);
    assert.equal(cell.querySelector(".ohm-table-cell-content").hidden, false);
  }
});

test("multiple collapse and expand orders preserve total width and recover original column widths", () => {
  for (const order of [[0, 1, 0, 1], [0, 1, 1, 0], [1, 0, 0, 1], [1, 0, 1, 0], [2, 0, 2, 0]]) {
    const app = fixture({ specs: [{ labels: ["A", "B", "C"], widths: [100, 200, 300] }] });
    for (const index of order) {
      app.toggle(index);
      close(app.tables[0].getBoundingClientRect().width, 600, "toggle retains table width");
      app.widths().forEach((width, column) => {
        if (app.toggleButton(column).getAttribute("aria-expanded") === "false") close(width, 32);
        else assert.ok(width >= 48);
      });
    }
    closeWidths(app.widths(), [100, 200, 300]);
    assert.deepEqual(app.saved()[tableKey(["A", "B", "C"])], [100, 200, 300]);
  }
});

test("one column must remain expanded and single-column tables have no collapse controls", () => {
  const app = fixture({ specs: [{ labels: ["A", "B", "C"], widths: [100, 200, 300] }] });
  app.toggle(0); app.toggle(1);
  closeWidths(app.widths(), [32, 32, 536]);
  const last = app.toggleButton(2), writes = app.storage.writes.length;
  assert.equal(last.disabled, true);
  assert.match(last.getAttribute("title"), /one|last/i);
  app.toggle(2);
  closeWidths(app.widths(), [32, 32, 536]);
  assert.equal(app.storage.writes.length, writes, "programmatic click cannot collapse the final expanded column");
  assert.equal(app.toggleButton(0).disabled, false);
  app.toggle(0);
  assert.equal(last.disabled, false, "another expanded column makes collapse available again");
  const single = fixture({ specs: [{ labels: ["Only"], widths: [600] }] });
  assert.equal(single.tables[0].querySelector(".ohm-table-column-toggle"), null);
  single.key(single.column(), "ArrowLeft");
  closeWidths(single.widths(), [599]);
});

test("collapse hides rich authored header, body and footer nodes without replacing them or their handlers", () => {
  let header, link, footerNode, headerText, linkClicks = 0;
  const app = fixture({ specs: [{ footer: [["Total", "2"]] }], beforeRun({ document, tables }) {
    const cells = tables[0].querySelectorAll("thead th, tbody td, tfoot td");
    header = document.createElement("strong");
    header.textContent = "Name";
    headerText = header.firstChild;
    cells[0].textContent = "";
    cells[0].appendChild(header);
    link = document.createElement("a");
    link.textContent = "Details";
    link.setAttribute("href", "details.html");
    link.setAttribute("data-authored", "keep");
    link.addEventListener("click", () => { linkClicks++; });
    cells[2].appendChild(link);
    footerNode = document.createElement("code");
    footerNode.textContent = "Authored footer";
    cells[4].appendChild(footerNode);
  } });
  link.focus();
  app.toggle();
  assert.equal(app.document.activeElement, app.toggleButton(), "focus moves to the restore control before focused content is hidden");
  assert.equal(header.firstChild, headerText, "original text node survives wrapping");
  for (const node of [header, link, footerNode]) {
    assert.equal(node.closest("table"), app.tables[0]);
    assert.equal(node.parentElement.className, "ohm-table-cell-content");
    assert.equal(hidden(node), true, "authored content and focusable links are hidden");
  }
  assert.equal(link.getAttribute("href"), "details.html");
  assert.equal(link.getAttribute("data-authored"), "keep");
  assert.equal(app.tables[0].querySelector(".ohm-table-controls-cell").classList.contains("ohm-table-column-collapsed"), false);
  assert.equal(hidden(app.frame().querySelector(".ohm-table-reset")), false);
  app.toggle();
  for (const node of [header, link, footerNode]) assert.equal(hidden(node), false);
  link.dispatchEvent({ type: "click" });
  assert.equal(linkClicks, 1, "authored listener remains attached after collapse and expand");
  assert.equal(header.firstChild, headerText);
});

test("collapsed grips reject keyboard and pointer resizing while expanded grips remain independent", () => {
  const app = fixture();
  app.toggle();
  const handle = app.column(), writes = app.storage.writes.length;
  assert.equal(handle.hidden, true);
  assert.equal(handle.getAttribute("aria-hidden"), "true");
  assert.equal(handle.tabIndex, -1);
  app.key(handle, "ArrowRight", true);
  app.down(handle, boundary(app)); app.move(boundary(app) + 100); app.up(boundary(app) + 100);
  closeWidths(app.widths(), [32, 568]);
  assert.equal(app.storage.writes.length, writes);
  assert.equal(app.document.documentElement.classList.contains("ohm-table-resizing"), false);
  app.key(app.column(1), "ArrowRight", true);
  closeWidths(app.widths(), [32, 578]);
  close(app.tables[0].getBoundingClientRect().width, 610);
  app.toggle();
  assert.ok(app.widths().every((width) => width >= 48));
  close(app.tables[0].getBoundingClientRect().width, 610, "restoring a column uses the current width budget");
  assert.equal(handle.hidden, false);
  assert.equal(handle.getAttribute("aria-hidden"), null);
  assert.equal(handle.tabIndex, 0);
  const restored = app.widths();
  app.key(handle, "ArrowRight");
  closeWidths(app.widths(), [restored[0] + 1, restored[1]]);
});

test("expanding grows the table only when expanded column minimums need more space", () => {
  const app = fixture();
  app.toggle();
  dragBy(app, app.column(1), -2000, boundary(app, 1));
  closeWidths(app.widths(), [32, 48]);
  close(app.tables[0].getBoundingClientRect().width, 80);
  app.toggle();
  closeWidths(app.widths(), [48, 48]);
  close(app.tables[0].getBoundingClientRect().width, 96);
});

test("collapsing during a drag cancels transient widths before applying the collapse", () => {
  const app = fixture();
  const start = boundary(app, 1);
  app.down(app.column(1), start);
  app.move(start + 50);
  assert.ok(app.widths()[1] > 400);
  app.toggle(0);
  closeWidths(app.widths(), [32, 568]);
  close(app.tables[0].getBoundingClientRect().width, 600);
  assert.equal(app.frame().querySelector(".ohm-table-drag-spacer"), null);
  assert.equal(app.document.documentElement.classList.contains("ohm-table-resizing"), false);
  app.move(start + 150); app.up(start + 150);
  closeWidths(app.widths(), [32, 568]);
  assert.equal(app.storage.writes.length, 1, "only committed collapse state is saved");
});

test("reset expands all columns and clears only the selected table's combined layout", () => {
  const app = fixture({ specs: [{}, {}] });
  app.toggle(0, 0);
  app.toggle(1, 1);
  const other = app.saved()[tableKey(undefined, 1)];
  app.reset(0);
  closeWidths(app.widths(0), [200, 400]);
  assert.equal(app.tables[0].classList.contains("ohm-table--fixed"), false);
  assert.equal(app.tables[0].querySelector("colgroup"), null);
  assert.equal(app.saved()[tableKey()], undefined);
  assert.deepEqual(app.saved()[tableKey(undefined, 1)], other);
  for (const cell of authoredCells(app, 0)) {
    assert.equal(cell.classList.contains("ohm-table-column-collapsed"), false);
    assert.equal(cell.querySelector(".ohm-table-cell-content").hidden, false);
  }
  assert.equal(app.toggleButton(0).getAttribute("aria-expanded"), "true");
  assert.equal(app.frame().querySelector(".ohm-table-reset").disabled, true);
});

test("collapsed layouts round trip through session storage without rewriting valid state", () => {
  const app = fixture();
  app.toggle();
  const saved = app.saved()[tableKey()];
  assert.deepEqual(saved, { widths: [32, 568], collapsed: [true, false], expanded: [200, 400] });
  const writes = app.storage.writes.length;
  const restored = fixture({ storage: app.storage });
  closeWidths(restored.widths(), [32, 568]);
  assert.equal(restored.toggleButton().getAttribute("aria-expanded"), "false");
  assert.equal(restored.column().hidden, true);
  assert.equal(app.storage.writes.length, writes);
  restored.toggle();
  closeWidths(restored.widths(), [200, 400]);
  assert.deepEqual(restored.saved()[tableKey()], [200, 400], "all-open state retains the legacy width-array shape");
  const again = fixture({ storage: app.storage });
  closeWidths(again.widths(), [200, 400]);
  const fresh = fixture();
  assert.equal(fresh.toggleButton().getAttribute("aria-expanded"), "true");
  assert.deepEqual(fresh.storage.writes, []);
});

test("old width arrays restore unchanged and become combined layouts only after collapsing", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [210, 390] })]]);
  const app = fixture({ storage });
  closeWidths(app.widths(), [210, 390]);
  assert.deepEqual(storage.writes, []);
  assert.equal(app.toggleButton().getAttribute("aria-expanded"), "true");
  app.toggle();
  assert.deepEqual(app.saved()[tableKey()], { widths: [32, 568], collapsed: [true, false], expanded: [210, 390] });
  app.toggle();
  closeWidths(app.widths(), [210, 390]);
});

test("resizing with a collapsed column persists rebased open widths that restore without cleanup", () => {
  const specs = [{ labels: ["A", "B", "C"], widths: [100, 200, 300] }];
  const app = fixture({ specs });
  app.toggle(0);
  const before = app.widths();
  app.key(app.column(1), "ArrowRight", true);
  const widths = app.widths();
  closeWidths(widths, [32, before[1] + 10, before[2]]);
  const saved = app.saved()[tableKey(["A", "B", "C"])];
  closeWidths(saved.expanded, [100, widths[1], widths[2]]);
  const writes = app.storage.writes.length;
  const restored = fixture({ specs, storage: app.storage });
  closeWidths(restored.widths(), widths);
  assert.equal(app.storage.writes.length, writes, "valid resized collapse state is restored without cleanup or migration");
  restored.toggle(0);
  assert.ok(restored.widths().every((width) => width >= 48));
  close(restored.tables[0].getBoundingClientRect().width, 610);
});

test("invalid combined layouts are discarded without hiding content or disabling collapse", () => {
  const valid = { widths: [32, 568], collapsed: [true, false], expanded: [200, 400] };
  const invalid = [
    { ...valid, collapsed: [true] }, { ...valid, collapsed: [1, false] },
    { ...valid, collapsed: [true, true] }, { ...valid, collapsed: [false, false] },
    { ...valid, collapsed: null }, { ...valid, widths: [32] },
    { ...valid, widths: [48, 552] }, { ...valid, widths: [32, 47] },
    { ...valid, widths: [32, "568"] }, { ...valid, widths: [32, null] },
    { ...valid, expanded: [200] }, { ...valid, expanded: [47, 400] },
    { ...valid, expanded: [200, "400"] }, { ...valid, expanded: [Number.MAX_VALUE, Number.MAX_VALUE] },
  ];
  for (const state of invalid) {
    const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: state })]]);
    const app = fixture({ storage });
    closeWidths(app.widths(), [200, 400]);
    assert.equal(app.toggleButton().getAttribute("aria-expanded"), "true");
    assert.equal(storage.entries.has(storageKey()), false);
    app.toggle();
    closeWidths(app.widths(), [32, 568]);
  }
  const labels = ["A", "B", "C"];
  const inconsistent = { widths: [32, 200, 368], collapsed: [true, false, false], expanded: [100, 200, 300] };
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey(labels)]: inconsistent })]]);
  const app = fixture({ specs: [{ labels, widths: [100, 200, 300] }], storage });
  closeWidths(app.widths(), [100, 200, 300]);
  assert.equal(storage.entries.has(storageKey()), false, "noncanonical rendered widths cannot corrupt remembered toggle proportions");
});

test("blocked session storage leaves collapse, visible resizing and reset usable", () => {
  for (const failure of ["get", "set", "remove", "getter"]) {
    const storage = new Storage();
    if (failure !== "getter") storage.block.add(failure);
    const app = fixture({ storage, blockedStorageGetter: failure === "getter" });
    app.toggle();
    closeWidths(app.widths(), [32, 568]);
    app.key(app.column(1), "ArrowRight");
    closeWidths(app.widths(), [32, 569]);
    app.reset();
    closeWidths(app.widths(), [200, 400]);
    assert.equal(app.toggleButton().getAttribute("aria-expanded"), "true");
  }
});

test("collapsed strips survive viewport changes and scrolled drag cancellation", () => {
  const storage = new Storage([[storageKey(), JSON.stringify({ [tableKey()]: [600, 600] })]]);
  const app = fixture({ storage, mainWidth: 540 });
  app.toggle();
  closeWidths(app.widths(), [32, 1168]);
  app.resize(440);
  closeWidths(app.widths(), [32, 1168]);
  close(app.scroll().clientWidth, 400);
  app.scroll().scrollLeft = 800;
  const start = boundary(app, 1), writes = storage.writes.length, saved = app.saved()[tableKey()];
  app.down(app.column(1), start); app.move(start - 10);
  closeWidths(app.widths(), [32, 1158]);
  app.cancel();
  closeWidths(app.widths(), [32, 1168]);
  close(app.scroll().scrollLeft, 800);
  assert.equal(storage.writes.length, writes);
  assert.deepEqual(app.saved()[tableKey()], saved, "cancel preserves remembered expanded widths as well as rendered tracks");
  assert.equal(app.toggleButton().getAttribute("aria-expanded"), "false");
  assert.equal(app.column().hidden, true);
  app.toggle();
  closeWidths(app.widths(), [600, 600]);
});

test("canceled, unchanged and returned-to-origin drags while collapsed preserve remembered expansion widths", () => {
  for (const outcome of ["unchanged", "returned", "canceled"]) {
    const app = fixture();
    app.toggle();
    const saved = app.saved()[tableKey()], writes = app.storage.writes.length;
    const start = boundary(app, 1);
    app.down(app.column(1), start);
    if (outcome !== "unchanged") app.move(start + 40);
    if (outcome === "canceled") app.cancel();
    else {
      if (outcome === "returned") app.move(start);
      app.up(start);
    }
    closeWidths(app.widths(), [32, 568]);
    assert.deepEqual(app.saved()[tableKey()], saved);
    assert.equal(app.storage.writes.length, writes);
    app.toggle();
    closeWidths(app.widths(), [200, 400]);
  }
});
