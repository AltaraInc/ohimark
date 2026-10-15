"use strict";

const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const script = readFileSync(join(__dirname, "../src/assets/ohimark.js"), "utf8");
const SITE = "https://example.test/docs/index.html";
const sortKey = (site = SITE) => "ohimark-sidebar-sort:" + site;
const collapseKey = (site = SITE) => "ohimark-sidebar-groups:" + site;

// This deliberately small DOM implements only the browser surface used by the
// bundled script. Sorting, preference parsing, and event handlers all run from
// the real asset; the fixture contains no parallel implementation of them.
function matchesSimple(element, selector) {
  let valid = true;
  selector = selector.replace(/\[([\w-]+)(?:(\^?=)"([^"]*)")?\]/g, (_, name, op, value) => {
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
      if (!node || !matchesSimple(node, parts[index])) return false;
      if (index === 0) return true;
      if (parts[index - 1] === ">") return at(node.parentElement, index - 2);
      for (let parent = node.parentElement; parent; parent = parent.parentElement) {
        if (at(parent, index - 2)) return true;
      }
      return false;
    }
    return at(element, parts.length - 1);
  });
}

class Element {
  constructor(tagName, ownerDocument) {
    this.tagName = tagName.toUpperCase();
    this.ownerDocument = ownerDocument;
    this.attributes = new Map();
    this.children = [];
    this.parentNode = null;
    this.listeners = new Map();
    this.style = {};
    this.open = false;
    this.checked = false;
    this._text = "";
  }
  get parentElement() { return this.parentNode; }
  get firstChild() { return this.children[0] || null; }
  get id() { return this.getAttribute("id") || ""; }
  set id(value) { this.setAttribute("id", value); }
  get className() { return this.getAttribute("class") || ""; }
  set className(value) { this.setAttribute("class", value); }
  get classList() {
    const element = this;
    return {
      contains(name) { return element.className.split(/\s+/).includes(name); },
      add(name) { if (!this.contains(name)) element.className = (element.className + " " + name).trim(); },
      remove(name) { element.className = element.className.split(/\s+/).filter((item) => item !== name).join(" "); },
      toggle(name, force) {
        const enabled = force === undefined ? !this.contains(name) : force;
        if (enabled) this.add(name); else this.remove(name);
        return enabled;
      },
    };
  }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  set textContent(value) {
    for (const child of [...this.children]) this.removeChild(child);
    this._text = String(value);
  }
  get innerHTML() { return this._html || ""; }
  set innerHTML(value) { this.textContent = ""; this._html = String(value); }
  get href() { return new URL(this.getAttribute("href"), this.ownerDocument.url).href; }
  get value() {
    if (this.tagName === "SELECT") return this._value ?? this.children[0]?.value ?? "";
    return this.getAttribute("value") || "";
  }
  set value(value) {
    if (this.tagName === "SELECT") {
      this._value = this.children.some((option) => option.value === value) ? value : "";
    } else this.setAttribute("value", value);
  }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  removeAttribute(name) { this.attributes.delete(name); }
  appendChild(child) { return this.insertBefore(child, null); }
  insertBefore(child, reference) {
    if (child.parentNode) child.parentNode.removeChild(child);
    const index = reference === null ? this.children.length : this.children.indexOf(reference);
    assert.ok(index >= 0, "insertBefore reference belongs to parent");
    this.children.splice(index, 0, child);
    child.parentNode = this;
    return child;
  }
  removeChild(child) {
    const index = this.children.indexOf(child);
    assert.ok(index >= 0, "removeChild node belongs to parent");
    this.children.splice(index, 1);
    child.parentNode = null;
    return child;
  }
  replaceChild(replacement, child) {
    this.insertBefore(replacement, child);
    return this.removeChild(child);
  }
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
  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(listener);
  }
  dispatchEvent(event) {
    event.target ??= this;
    event.defaultPrevented ??= false;
    event.stopPropagation = () => { event.stopped = true; };
    event.preventDefault = () => { event.defaultPrevented = true; };
    for (let node = this; node; node = event.bubbles === false ? null : node.parentNode) {
      for (const listener of node.listeners.get(event.type) || []) listener.call(node, event);
      if (event.stopped) break;
    }
    return !event.defaultPrevented;
  }
  focus() { this.ownerDocument.activeElement = this; }
}

class Storage {
  constructor(entries = []) {
    this.entries = new Map(entries);
    this.calls = [];
    this.block = new Set();
  }
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

function fixture({ pages = [], groups = [], site = SITE, url = SITE, mobile = false, storage = new Storage(), beforeRun } = {}) {
  const document = new Element("#document");
  document.ownerDocument = document;
  document.url = url;
  document.createElement = (tag) => new Element(tag, document);
  document.getElementById = (id) => document.querySelector("#" + id);
  const create = (tag, classes, parent, text = "") => {
    const element = document.createElement(tag);
    element.className = classes;
    element.textContent = text;
    if (parent) parent.appendChild(element);
    return element;
  };
  document.documentElement = create("html", "", document);
  document.body = create("body", "", document.documentElement);
  const brand = create("a", "ohm-brand", document.body, "Site");
  brand.setAttribute("href", site);
  const navState = create("input", "ohm-nav-state", document.body);
  navState.id = "ohm-nav-state";
  const sidebar = create("aside", "ohm-sidebar", document.body);
  sidebar.id = "ohm-sidebar";
  const nav = create("nav", "ohm-nav", sidebar);
  const header = create("div", "ohm-panel-header ohm-sidebar-header", nav);
  const root = create("a", "ohm-nav-link", header, "Pinned home");
  root.setAttribute("href", "index.html");
  const panelLabel = create("label", "ohm-panel-button ohm-nav-button", header);
  panelLabel.setAttribute("for", "ohm-nav-state");
  const top = create("ul", "ohm-nav-list", nav);
  const lists = new Map([["", top]]);
  const details = new Map();
  const rows = new Map();
  function populate(list, pageSpecs, groupSpecs) {
    for (const page of pageSpecs) {
      const row = create("li", "ohm-nav-item", list);
      row.id = page.id;
      for (const field of ["created", "updated"]) {
        if (page[field] !== undefined) row.setAttribute("data-ohm-" + field, page[field]);
      }
      const link = create("a", "ohm-nav-link", row, page.title);
      link.setAttribute("href", page.href || page.id + ".html");
      rows.set(page.id, row);
    }
    for (const group of groupSpecs) {
      const row = create("li", "ohm-nav-group", list);
      row.id = "group:" + group.key;
      for (const field of ["created", "updated"]) {
        if (group[field] !== undefined) row.setAttribute("data-ohm-" + field, group[field]);
      }
      const disclosure = create("details", "ohm-nav-details", row);
      disclosure.setAttribute("data-ohm-nav-group", group.key);
      disclosure.open = group.open !== false;
      const summary = create("summary", "ohm-nav-summary", disclosure);
      const label = create(group.linked ? "a" : "span", "ohm-nav-label" + (group.linked ? " ohm-nav-link" : ""), summary, group.title || group.key);
      if (group.linked) label.setAttribute("href", group.key + "/README.html");
      const children = create("ul", "ohm-nav-list", disclosure);
      lists.set(group.key, children);
      details.set(group.key, disclosure);
      populate(children, group.pages || [], group.groups || []);
    }
  }
  populate(top, pages, groups);
  const result = { document, storage, header, root, navState, lists, details, rows };
  if (beforeRun) beforeRun(result);
  const media = { matches: mobile, addEventListener() {} };
  const window = { sessionStorage: storage, innerWidth: mobile ? 600 : 1400, matchMedia: () => media, addEventListener() {} };
  vm.runInNewContext(script, { document, window, navigator: {}, URL, setTimeout }, { filename: "ohimark.js" });
  result.select = (key = "") => {
    const parent = key === "" ? header : details.get(key).parentElement;
    return parent.children.find((child) => child.classList.contains("ohm-nav-sort"))?.querySelector("select") || null;
  };
  result.order = (key = "") => lists.get(key).children.map((child) => child.id);
  result.choose = (key, mode) => {
    const select = result.select(key);
    assert.ok(select, "sort chooser exists for " + key);
    select.value = mode;
    select.dispatchEvent({ type: "change" });
  };
  return result;
}

const pages = [
  { id: "ten", title: "Page 10", href: "a.html", created: "0", updated: "10" },
  { id: "two", title: "page 2", href: "z.html", created: "-5", updated: "0" },
  { id: "two-case", title: "PAGE 2", href: "x.html", created: "-5", updated: "0" },
  { id: "alpha", title: "Alpha", created: "20", updated: "-20" },
  { id: "unknown", title: "Zebra" },
];

test("all modes use visible natural titles, stable ties, dates, and original default order", () => {
  const app = fixture({ pages });
  const expected = {
    "name-asc": ["alpha", "two", "two-case", "ten", "unknown"],
    "name-desc": ["unknown", "ten", "two", "two-case", "alpha"],
    "created-desc": ["alpha", "ten", "two", "two-case", "unknown"],
    "created-asc": ["two", "two-case", "ten", "alpha", "unknown"],
    "updated-desc": ["ten", "two", "two-case", "alpha", "unknown"],
    "updated-asc": ["alpha", "two", "two-case", "ten", "unknown"],
    default: pages.map((page) => page.id),
  };
  assert.equal(app.select().children.length, 7);
  for (const [mode, order] of Object.entries(expected)) {
    app.choose("", mode);
    assert.deepEqual(app.order(), order, mode);
    assert.equal(app.select().value, mode);
    assert.equal(app.select().parentElement.getAttribute("data-ohm-sort-mode"), mode);
    assert.equal(app.select().parentElement.classList.contains("ohm-nav-sort--active"), mode !== "default");
    assert.ok(app.select().getAttribute("title").includes(app.select().children.find((option) => option.value === mode).textContent));
  }
  assert.equal(app.storage.entries.has(sortKey()), false);
  assert.equal(app.header.firstChild, app.root);
  assert.equal(app.root.textContent, "Pinned home");
});

test("timestamps accept epoch zero and negatives; invalid, absent, and unsafe values remain last", () => {
  const records = [
    ["unknown", undefined], ["empty", ""], ["fraction", "1.5"], ["space", " 0"],
    ["exponent", "1e3"], ["unsafe", "9007199254740992"], ["infinite", "Infinity"],
    ["nan", "NaN"], ["zero", "0"], ["negative", "-12"], ["positive", "40"],
  ];
  const app = fixture({ pages: records.map(([id, value]) => ({ id, title: id, created: value, updated: value })) });
  const unknown = ["empty", "exponent", "fraction", "infinite", "nan", "space", "unknown", "unsafe"];
  for (const field of ["created", "updated"]) {
    app.choose("", field + "-asc");
    assert.deepEqual(app.order(), ["negative", "zero", "positive", ...unknown]);
    app.choose("", field + "-desc");
    assert.deepEqual(app.order(), ["positive", "zero", "negative", ...unknown]);
  }
});

test("equal and unknown dates break ties by natural title then original order", () => {
  const app = fixture({ pages: [
    { id: "z", title: "z", created: "2" }, { id: "a10", title: "a10", created: "2" },
    { id: "a2", title: "a2", created: "2" }, { id: "A2", title: "A2", created: "2" },
    { id: "u10", title: "unknown10" }, { id: "u2", title: "Unknown2" },
  ] });
  for (const mode of ["created-asc", "created-desc"]) {
    app.choose("", mode);
    assert.deepEqual(app.order(), ["a2", "A2", "a10", "z", "u2", "u10"]);
  }
});

test("pages and subgroups interleave by natural name and dates, with stable shared ties and exact default restoration", () => {
  const app = fixture({ pages: [
    { id: "p10", title: "Item 10", created: "0", updated: "50" },
    { id: "p2", title: "item 2", created: "10", updated: "-10" },
    { id: "pu", title: "Unknown 2" },
  ], groups: [
    { key: "folder-z", title: "Item 3", created: "-10", updated: "10" },
    { key: "folder-a", title: "ITEM 2", linked: true, created: "10", updated: "-10", pages: [{ id: "descendant", title: "Ignore this descendant's title" }] },
    { key: "unknown", title: "Unknown 10" },
    { key: "invalid", title: "Unknown 3", created: "", updated: "9007199254740992" },
  ], beforeRun({ lists }) {
    const list = lists.get("");
    list.insertBefore(list.children.find((row) => row.id === "group:folder-a"), list.children[1]);
    list.insertBefore(list.children.find((row) => row.id === "group:folder-z"), list.children[3]);
  } });
  const unknown = ["pu", "group:invalid", "group:unknown"];
  const expected = {
    "name-asc": ["group:folder-a", "p2", "group:folder-z", "p10", ...unknown],
    "name-desc": ["group:unknown", "group:invalid", "pu", "p10", "group:folder-z", "group:folder-a", "p2"],
    "created-asc": ["group:folder-z", "p10", "group:folder-a", "p2", ...unknown],
    "created-desc": ["group:folder-a", "p2", "p10", "group:folder-z", ...unknown],
    "updated-asc": ["group:folder-a", "p2", "group:folder-z", "p10", ...unknown],
    "updated-desc": ["p10", "group:folder-z", "group:folder-a", "p2", ...unknown],
    default: ["p10", "group:folder-a", "p2", "group:folder-z", "pu", "group:unknown", "group:invalid"],
  };
  assert.deepEqual(app.order(), expected.default);
  for (const [mode, order] of Object.entries(expected)) {
    app.choose("", mode);
    assert.deepEqual(app.order(), order, mode);
    assert.deepEqual(app.order("folder-a"), ["descendant"], "parent sorting never sorts descendants");
  }
  assert.equal(app.header.firstChild, app.root);
});

test("chooser eligibility counts both pages and groups at every level", () => {
  for (const options of [{}, { pages: pages.slice(0, 1) }, { groups: [{ key: "only" }] }]) {
    assert.equal(fixture(options).select(), null, "zero or one total row has no chooser");
  }
  const subgroupOnly = fixture({ groups: [{ key: "z", title: "Z" }, { key: "a", title: "A" }] });
  subgroupOnly.choose("", "name-asc");
  assert.deepEqual(subgroupOnly.order(), ["group:a", "group:z"]);
  const mixed = fixture({ pages: [{ id: "page", title: "Z" }], groups: [
    { key: "parent", title: "A", pages: [{ id: "nested", title: "Z" }], groups: [{ key: "parent/sub", title: "A" }] },
  ] });
  mixed.choose("", "name-asc");
  mixed.choose("parent", "name-asc");
  assert.deepEqual(mixed.order(), ["group:parent", "page"]);
  assert.deepEqual(mixed.order("parent"), ["group:parent/sub", "nested"]);
  assert.equal(mixed.select("parent/sub"), null);
});

test("parent group moves preserve nested choices, collapsed state, current markers, and attached listeners across reloads", () => {
  const storage = new Storage([[collapseKey(), '["docs","docs/sub"]']]);
  const options = { storage, pages: [{ id: "middle", title: "M page" }], groups: [
    { key: "docs", title: "A group", linked: true, pages: [{ id: "nested-z", title: "Z" }, { id: "nested-a", title: "A" }], groups: [
      { key: "docs/sub", title: "B subgroup", pages: [{ id: "sub-z", title: "Z" }, { id: "sub-a", title: "A" }] },
    ] },
    { key: "other", title: "Z group", pages: [{ id: "other-z", title: "Z" }, { id: "other-a", title: "A" }] },
  ] };
  let clicked = 0;
  const app = fixture({ ...options, beforeRun({ details }) {
    const link = details.get("docs").querySelector("summary > a");
    link.classList.add("ohm-nav-link--current");
    link.setAttribute("aria-current", "page");
    link.addEventListener("click", () => clicked++);
  } });
  const docs = app.details.get("docs");
  const row = docs.parentElement;
  const chooser = app.select("docs");
  const current = docs.querySelector("summary > a");
  app.choose("", "name-asc");
  assert.deepEqual(app.order(), ["group:docs", "middle", "group:other"]);
  assert.deepEqual(app.order("docs"), ["nested-z", "nested-a", "group:docs/sub"]);
  app.choose("docs", "name-asc");
  app.choose("docs/sub", "name-asc");
  app.choose("", "name-desc");
  assert.deepEqual(app.order(), ["group:other", "middle", "group:docs"]);
  assert.deepEqual(app.order("docs"), ["nested-a", "group:docs/sub", "nested-z"]);
  assert.deepEqual(app.order("docs/sub"), ["sub-a", "sub-z"]);
  assert.deepEqual(app.order("other"), ["other-z", "other-a"]);
  assert.equal(app.details.get("docs").parentElement, row);
  assert.equal(app.select("docs"), chooser);
  assert.equal(docs.querySelector("summary > a"), current);
  assert.equal(current.getAttribute("aria-current"), "page");
  assert.equal(current.classList.contains("ohm-nav-link--current"), true);
  current.dispatchEvent({ type: "click" });
  assert.equal(clicked, 1);
  for (const key of ["docs", "docs/sub"]) {
    assert.equal(app.details.get(key).open, false);
    app.details.get(key).dispatchEvent({ type: "toggle", bubbles: false });
  }
  assert.equal(storage.entries.get(collapseKey()), '["docs","docs/sub"]');
  assert.deepEqual(JSON.parse(storage.entries.get(sortKey())), { "": "name-desc", docs: "name-asc", "docs/sub": "name-asc" });
  const restored = fixture(options);
  assert.deepEqual(restored.order(), app.order());
  assert.deepEqual(restored.order("docs"), app.order("docs"));
  assert.deepEqual(restored.order("docs/sub"), app.order("docs/sub"));
  assert.equal(restored.select("docs").value, "name-asc");
  assert.equal(restored.details.get("docs").open, false);
  assert.equal(restored.details.get("docs/sub").open, false);
  restored.choose("", "default");
  assert.deepEqual(restored.order(), ["middle", "group:docs", "group:other"]);
  assert.deepEqual(restored.order("docs"), ["nested-a", "group:docs/sub", "nested-z"]);
});

test("reordering prefers state-preserving native moves, falls back safely, and skips unchanged rows", () => {
  for (const nativeMoves of [true, false]) {
    let moved = 0;
    let inserted = 0;
    const app = fixture({ pages: [{ id: "page", title: "Z" }], groups: [{ key: "group", title: "A" }], beforeRun({ lists }) {
      const list = lists.get("");
      const insertBefore = list.insertBefore;
      list.insertBefore = function (child, reference) {
        inserted++;
        return insertBefore.call(this, child, reference);
      };
      if (nativeMoves) list.moveBefore = function (child, reference) {
        moved++;
        assert.equal(child.parentElement, this);
        this.children.splice(this.children.indexOf(child), 1);
        const index = reference === null ? this.children.length : this.children.indexOf(reference);
        assert.ok(index >= 0);
        this.children.splice(index, 0, child);
      };
    } });
    assert.equal(moved + inserted, 0, "default initialization does not move rows");
    app.choose("", "name-asc");
    assert.deepEqual(app.order(), ["group:group", "page"]);
    assert.equal(nativeMoves ? moved : inserted, 1);
    assert.equal(nativeMoves ? inserted : moved, 0);
    app.choose("", "name-asc");
    assert.equal(moved + inserted, 1, "reapplying the same order does not move rows");
    app.choose("", "default");
    assert.deepEqual(app.order(), ["page", "group:group"]);
    assert.equal(moved + inserted, 2);
  }
});

test("preferences restore across nested navigation, remain site-specific, and reset independently", () => {
  const storage = new Storage();
  const groups = [{ key: "notes", title: "Notes", pages: [{ id: "z", title: "Z" }, { id: "a", title: "A" }] }];
  const first = fixture({ pages, groups, storage });
  first.choose("", "created-asc");
  first.choose("notes", "name-asc");
  const nested = fixture({ pages, groups, storage, site: "../index.html", url: "https://example.test/docs/notes/page.html" });
  assert.equal(nested.select().value, "created-asc");
  assert.deepEqual(nested.order("notes"), ["a", "z"]);
  const secondSite = fixture({ pages, groups, storage, site: "https://example.test/other/index.html" });
  assert.equal(secondSite.select().value, "default");
  secondSite.choose("", "name-desc");
  nested.choose("", "default");
  assert.deepEqual(JSON.parse(storage.entries.get(sortKey())), { notes: "name-asc" });
  nested.choose("notes", "default");
  assert.equal(storage.entries.has(sortKey()), false);
  assert.deepEqual(JSON.parse(storage.entries.get(sortKey("https://example.test/other/index.html"))), { "": "name-desc" });
});

test("prototype-sensitive directory paths safely persist and restore as own keys", () => {
  const keys = ["__proto__", "constructor", "toString", "hasOwnProperty"];
  const groups = keys.map((key, index) => ({ key, pages: [{ id: index + "z", title: "Z" }, { id: index + "a", title: "A" }] }));
  const app = fixture({ groups });
  for (const key of keys) app.choose(key, "name-asc");
  assert.deepEqual(JSON.parse(app.storage.entries.get(sortKey())), Object.fromEntries(keys.map((key) => [key, "name-asc"])));
  const restored = fixture({ groups, storage: app.storage });
  for (const [index, key] of keys.entries()) {
    assert.equal(restored.select(key).value, "name-asc");
    assert.deepEqual(restored.order(key), [index + "a", index + "z"]);
  }
  restored.choose("__proto__", "default");
  assert.equal(Object.hasOwn(JSON.parse(app.storage.entries.get(sortKey())), "__proto__"), false);
});

test("preferences for extant groups with fewer than two rows survive while stale and invalid values are cleaned", () => {
  const storage = new Storage([[sortKey(), JSON.stringify({ "": "default", one: "name-desc", zero: "updated-asc", gone: "name-asc", invalid: "random", wrongType: 4 })]]);
  const app = fixture({ pages: pages.slice(0, 1), groups: [
    { key: "one", pages: [{ id: "single", title: "Only" }] }, { key: "zero" }, { key: "invalid" }, { key: "wrongType" },
  ], storage });
  assert.equal(app.select().value, "default");
  for (const key of ["one", "zero", "invalid", "wrongType"]) assert.equal(app.select(key), null);
  assert.deepEqual(JSON.parse(storage.entries.get(sortKey())), { one: "name-desc", zero: "updated-asc" });
  assert.equal(app.details.get("one").parentElement.classList.contains("ohm-nav-group--sortable"), false);
  const sparseStorage = new Storage([[sortKey(), '{"":"name-desc","one":"created-asc"}']]);
  const sparse = fixture({ groups: [{ key: "one" }], storage: sparseStorage });
  assert.equal(sparse.select(), null);
  assert.equal(sparse.select("one"), null);
  assert.deepEqual(sparseStorage.writes, []);
  assert.deepEqual(JSON.parse(sparseStorage.entries.get(sortKey())), { "": "name-desc", one: "created-asc" });
});

test("clean first visits perform no storage writes", () => {
  const app = fixture({ pages, groups: [{ key: "docs", pages }] });
  assert.deepEqual(app.storage.writes, []);
  assert.deepEqual(app.order(), pages.map((page) => page.id).concat("group:docs"));
});

test("corrupt storage and invalid top-level structures are removed without disabling controls", () => {
  for (const invalid of ["{", "null", "[]", '"name-asc"', "true", "42", "{}", '{"":"bogus"}']) {
    const storage = new Storage([[sortKey(), invalid]]);
    const app = fixture({ pages, storage });
    assert.equal(app.select().value, "default", invalid);
    assert.equal(storage.entries.has(sortKey()), false, invalid);
    app.choose("", "name-asc");
    assert.deepEqual(app.order(), ["alpha", "two", "two-case", "ten", "unknown"]);
  }
});

test("blocked storage reads, writes, and removals leave sorting and panel controls working", () => {
  for (const operation of ["get", "set", "remove"]) {
    const storage = new Storage([[sortKey(), '{"":"name-desc"}']]);
    storage.block.add(operation);
    const app = fixture({ pages, storage });
    app.choose("", "name-asc");
    assert.deepEqual(app.order(), ["alpha", "two", "two-case", "ten", "unknown"]);
    app.choose("", "default");
    assert.deepEqual(app.order(), pages.map((page) => page.id));
    const navButton = app.document.querySelector("button.ohm-nav-button");
    navButton.dispatchEvent({ type: "click" });
    assert.equal(app.navState.checked, true);
    assert.equal(navButton.getAttribute("aria-expanded"), "false");
  }
});

test("choosers are accessible siblings of details, usable when closed, and preserve native events", () => {
  const storage = new Storage([[collapseKey(), '["docs"]']]);
  const app = fixture({ pages, groups: [{ key: "docs", title: "Long group title", linked: true, pages }], storage, mobile: true });
  const disclosure = app.details.get("docs");
  const select = app.select("docs");
  const control = select.parentElement;
  assert.equal(disclosure.open, false);
  assert.equal(control.parentElement, disclosure.parentElement);
  assert.equal(disclosure.parentElement.firstChild, control, "sort precedes the group's page links in keyboard order");
  assert.equal(select.closest("summary"), null);
  assert.equal(select.getAttribute("aria-label"), "Sort items in Long group title");
  assert.equal(app.select().getAttribute("aria-label"), "Sort top-level items");
  assert.ok(control.innerHTML.includes('<svg class="ohm-icon"'));
  assert.ok(app.header.children.indexOf(app.select().parentElement) < app.header.children.indexOf(app.document.querySelector(".ohm-nav-button")));
  let escaped = 0;
  for (const type of ["click", "keydown", "keyup"]) app.document.addEventListener(type, () => escaped++);
  app.navState.checked = true;
  for (const [type, key] of [["click"], ["keydown", " "], ["keyup", " "], ["keydown", "Enter"], ["keydown", "ArrowDown"], ["keydown", "Escape"]]) {
    const event = { type, key };
    assert.equal(select.dispatchEvent(event), true, "native event default is preserved");
    assert.equal(event.defaultPrevented, false);
    assert.equal(disclosure.open, false);
    assert.equal(app.navState.checked, true);
  }
  assert.equal(escaped, 0);
  app.choose("docs", "name-asc");
  assert.equal(disclosure.open, false);
  assert.equal(storage.entries.get(collapseKey()), '["docs"]');
});

test("sort persistence and group/panel collapsed preferences remain independent", () => {
  const storage = new Storage([[collapseKey(), '["docs"]'], ["ohimark-sidebar-collapsed", "true"]]);
  const app = fixture({ pages, groups: [{ key: "docs", pages }], storage });
  app.choose("docs", "created-desc");
  assert.equal(app.navState.checked, true);
  assert.equal(app.details.get("docs").open, false);
  const sorted = storage.entries.get(sortKey());
  const group = app.details.get("docs");
  group.open = true;
  group.dispatchEvent({ type: "toggle", bubbles: false });
  app.document.querySelector(".ohm-nav-button").dispatchEvent({ type: "click" });
  assert.equal(storage.entries.get(sortKey()), sorted);
  assert.equal(storage.entries.has(collapseKey()), false);
  assert.equal(storage.entries.get("ohimark-sidebar-collapsed"), "false");
});

test("sorting moves original rows and links without losing current markers or attached listeners", () => {
  let clicked = 0;
  let row;
  let link;
  const app = fixture({ pages, beforeRun({ rows }) {
    row = rows.get("two");
    link = row.firstChild;
    link.classList.add("ohm-nav-link--current");
    link.setAttribute("aria-current", "page");
    link.addEventListener("click", () => clicked++);
  } });
  for (const mode of ["name-asc", "created-desc", "default"]) app.choose("", mode);
  assert.equal(app.rows.get("two"), row);
  assert.equal(app.lists.get("").children[1], row);
  assert.equal(row.firstChild, link);
  assert.equal(link.getAttribute("aria-current"), "page");
  assert.equal(link.classList.contains("ohm-nav-link--current"), true);
  link.dispatchEvent({ type: "click" });
  assert.equal(clicked, 1);
});
