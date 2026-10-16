"use strict";

const assert = require("node:assert/strict");
const { readFileSync } = require("node:fs");
const { join } = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const script = readFileSync(join(__dirname, "../src/assets/ohimark.js"), "utf8");
const SITE = "https://example.test/docs/index.html";
const sortKey = (site = SITE) => "ohimark-todo-sort:" + site;

// A focused DOM surface, with all filtering and outline behavior supplied by
// the actual bundle. There is no duplicate filter implementation in the tests.
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

class Element {
  constructor(tagName, ownerDocument) {
    this.tagName = tagName.toUpperCase();
    this.ownerDocument = ownerDocument;
    this.attributes = new Map();
    this.children = [];
    this.parentNode = null;
    this.listeners = new Map();
    this.style = {};
    this.checked = false;
    this.disabled = false;
    this._text = "";
  }
  get parentElement() { return this.parentNode; }
  get firstChild() { return this.children[0] || null; }
  get nextSibling() {
    return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] || null;
  }
  get id() { return this.getAttribute("id") || ""; }
  set id(value) { this.setAttribute("id", value); }
  get type() { return this.getAttribute("type") || ""; }
  set type(value) { this.setAttribute("type", value); }
  get href() { return new URL(this.getAttribute("href"), this.ownerDocument.url).href; }
  get value() {
    if (this.tagName === "SELECT") return this._value ?? this.children[0]?.value ?? "";
    return this.getAttribute("value") || "";
  }
  set value(value) {
    if (this.tagName === "SELECT") this._value = this.children.some((option) => option.value === value) ? value : "";
    else this.setAttribute("value", value);
  }
  get className() { return this.getAttribute("class") || ""; }
  set className(value) { this.setAttribute("class", value); }
  get classList() {
    const element = this;
    return {
      contains(name) { return element.className.split(/\s+/).includes(name); },
      add(name) { if (!this.contains(name)) element.className = (element.className + " " + name).trim(); },
      remove(name) { element.className = element.className.split(/\s+/).filter((item) => item !== name).join(" "); },
      toggle(name, force = !this.contains(name)) {
        if (force) this.add(name); else this.remove(name);
        return force;
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
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  hasAttribute(name) { return this.attributes.has(name); }
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
    event.preventDefault = () => { event.defaultPrevented = true; };
    event.stopPropagation = () => { event.stopped = true; };
    for (let node = this; node; node = event.bubbles === false ? null : node.parentNode) {
      for (const listener of node.listeners.get(event.type) || []) listener.call(node, event);
      if (event.stopped) break;
    }
    return !event.defaultPrevented;
  }
  focus() {
    const previous = this.ownerDocument.activeElement;
    if (previous === this) return;
    this.ownerDocument.activeElement = this;
    if (previous) {
      previous.dispatchEvent({ type: "blur", bubbles: false, relatedTarget: this });
      previous.dispatchEvent({ type: "focusout", relatedTarget: this });
    }
    this.dispatchEvent({ type: "focus", bubbles: false, relatedTarget: previous });
    this.dispatchEvent({ type: "focusin", relatedTarget: previous });
  }
  scrollIntoView(options) { this.lastScroll = options; }
}

class Storage {
  constructor(entries = []) {
    this.entries = new Map(entries);
    this.calls = [];
    this.writes = [];
    this.block = new Set();
  }
  getItem(key) {
    this.calls.push(["get", key]);
    if (this.block.has("get")) throw new Error("Storage read blocked");
    return this.entries.get(key) ?? null;
  }
  setItem(key, value) {
    const call = ["set", key, value];
    this.calls.push(call);
    this.writes.push(call);
    if (this.block.has("set")) throw new Error("Storage write blocked");
    this.entries.set(key, value);
  }
  removeItem(key) {
    const call = ["remove", key];
    this.calls.push(call);
    this.writes.push(call);
    if (this.block.has("remove")) throw new Error("Storage removal blocked");
    this.entries.delete(key);
  }
}

function fixture({ scoped = true, title = true, filename = true, own = [], groups = [], collected = false, outside = [], observer = false, site = SITE, url = "https://example.test/TODO.html", storage = new Storage(), beforeRun } = {}) {
  const document = new Element("#document");
  document.ownerDocument = document;
  document.url = url;
  document.createElement = (tag) => new Element(tag, document);
  document.createElementNS = (_, tag) => document.createElement(tag);
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
  if (site !== null) create("a", "ohm-brand", document.body, "Site").setAttribute("href", site);
  const article = create("article", "ohm-content", document.body);
  if (scoped) article.setAttribute("data-ohm-todo", "");
  const headings = new Map();
  function heading(tag, id, parent) {
    const element = create(tag, "", parent, id);
    element.id = id;
    headings.set(id, element);
    return element;
  }
  if (title) heading("h1", "todo-title", article);
  if (filename) create("div", "ohm-page-filename", article, "TODO.md");
  const prose = create("p", "", article, "Authored TODO introduction.");
  heading("h2", "own-heading", article);
  const tasks = new Map();
  const tagLabels = new Map();
  function populate(specs, parent) {
    const list = create("ul", "", parent);
    for (const spec of specs) {
      const row = create("li", "ohm-task", list);
      row.id = spec.id;
      const paragraph = create("p", "", row, spec.id + " ");
      const checkbox = create("input", "", paragraph);
      checkbox.type = "checkbox";
      checkbox.checked = !!spec.checked;
      checkbox.disabled = true;
      if (spec.tags) {
        const tags = spec.tags.map((tag) => Array.isArray(tag) ? tag : [tag, tag]);
        row.setAttribute("data-ohm-tags", JSON.stringify(tags.map(([key]) => key)));
        const badges = create("span", "ohm-task-tags", paragraph);
        for (const [key, label, color] of tags) {
          const badge = create("span", "ohm-task-tag", badges, label);
          if (color !== undefined) badge.setAttribute("data-ohm-tag-color", color);
          if (!tagLabels.has(key.toLowerCase())) tagLabels.set(key.toLowerCase(), label);
        }
      }
      if (spec.metadata !== undefined) row.setAttribute("data-ohm-tags", spec.metadata);
      if (spec.heading) heading("h3", spec.heading, row);
      tasks.set(spec.id, row);
      if (spec.children) populate(spec.children, row);
    }
    return list;
  }
  populate(own, article);
  let collectedSection;
  if (collected) {
    collectedSection = create("section", "", article);
    collectedSection.setAttribute("data-ohm-todo-collected", "");
    heading("h1", "collected-heading", collectedSection);
  }
  const groupElements = new Map();
  for (const group of groups) {
    const section = create("section", "", collectedSection || article);
    section.setAttribute("data-ohm-todo-group", "");
    for (const field of ["created", "updated"]) {
      if (group[field] !== undefined) section.setAttribute("data-ohm-" + field, group[field]);
    }
    const fileHeading = heading("h2", "group-" + group.id, section);
    if (group.title !== undefined) fileHeading.textContent = group.title;
    populate(group.tasks, section);
    groupElements.set(group.id, section);
  }
  if (outside.length) populate(outside, create("article", "ohm-content", document.body));
  const outline = create("ul", "ohm-outline-list", document.body);
  const outlineRows = new Map();
  for (const id of headings.keys()) {
    const row = create("li", "ohm-outline-item", outline);
    create("a", "", row, id).setAttribute("href", "#" + id);
    outlineRows.set(id, row);
  }
  const observers = [];
  class Observer {
    constructor(callback) {
      this.callback = callback;
      this.observed = [];
      this.disconnected = false;
      observers.push(this);
    }
    observe(element) { this.observed.push(element); }
    disconnect() { this.disconnected = true; this.observed = []; }
    deliver(...elements) { this.callback(elements.map((target) => ({ target, isIntersecting: true }))); }
  }
  const window = {
    innerWidth: 1400,
    location: { href: url },
    sessionStorage: storage,
    addEventListener() {},
  };
  if (observer) window.IntersectionObserver = Observer;
  const app = { document, article, prose, tasks, headings, groups: groupElements, collected: collectedSection, outline, outlineRows, observers, storage, storageWrites: storage.writes, window };
  if (beforeRun) beforeRun(app);
  vm.runInNewContext(script, { document, window, navigator: {}, setTimeout }, { filename: "ohimark.js" });
  app.filter = document.querySelector(".ohm-todo-filter");
  app.sortSelect = document.querySelector(".ohm-todo-sort-select");
  app.sort = (mode) => {
    app.sortSelect.value = mode;
    app.sortSelect.dispatchEvent({ type: "change" });
  };
  app.groupOrder = () => (collectedSection || article).children.filter((element) => element.hasAttribute("data-ohm-todo-group"))
    .map((element) => element.querySelector("h2").id.slice("group-".length));
  app.outlineOrder = () => outline.children.map((row) => row.querySelector("a")?.getAttribute("href").slice(1) || row.id);
  app.input = app.filter?.querySelector(".ohm-todo-search");
  app.popup = app.filter?.querySelector(".ohm-todo-popup");
  app.rows = () => app.filter?.querySelectorAll(".ohm-todo-option") || [];
  app.labels = () => app.rows().map((row) => row.querySelector(".ohm-task-tag").textContent);
  app.selectedTags = () => app.filter?.querySelectorAll(".ohm-todo-selected-tag") || [];
  app.open = () => { app.input.focus(); app.input.dispatchEvent({ type: "click" }); };
  app.query = (value) => {
    app.input.focus();
    app.input.value = value;
    app.input.dispatchEvent({ type: "input" });
  };
  app.key = (key, properties = {}) => {
    const event = { type: "keydown", key, ...properties };
    app.input.dispatchEvent(event);
    return event;
  };
  app.choose = (key, selected = true) => {
    const label = tagLabels.get(key) || key;
    app.query(label);
    const option = app.rows().find((row) => row.querySelector(".ohm-task-tag").textContent === label);
    assert.ok(option, "tag suggestion exists for " + key);
    if ((option.getAttribute("aria-selected") === "true") !== selected) option.dispatchEvent({ type: "click" });
  };
  app.clear = () => app.filter.querySelector(".ohm-todo-clear").dispatchEvent({ type: "click" });
  app.count = () => app.filter.querySelector(".ohm-todo-count").getAttribute("aria-label");
  app.visibleTasks = () => [...tasks].filter(([, row]) => !row.closest("[hidden]")).map(([id]) => id);
  app.hiddenContent = () => document.querySelectorAll("[hidden]").filter((element) => !app.filter?.contains(element));
  return app;
}

test("controls require a final TODO scope and at least one valid tag", () => {
  for (const options of [
    { scoped: false, own: [{ id: "ordinary", tags: ["travel"] }] },
    { own: [{ id: "untagged" }] },
    { own: [{ id: "empty", tags: [] }] },
    { own: [{ id: "invalid", metadata: "{not json" }] },
    { own: [{ id: "invalid-shape", metadata: '{"travel":true}' }] },
    { own: [{ id: "invalid-keys", metadata: '[null,4,""]' }] },
    { outside: [{ id: "outside", tags: ["travel"] }] },
  ]) {
    const app = fixture(options);
    assert.equal(app.filter, null);
    assert.equal(app.hiddenContent().length, 0);
  }
});

test("catalog includes checked entries, uses canonical keys and first own spelling, and sorts labels", () => {
  const hostile = '<img src=x onerror="alert(1)">';
  const app = fixture({
    own: [
      { id: "checked", checked: true, tags: [["travel", "Travel"]] },
      { id: "parent", tags: [["buy", "Buy"]], children: [
        { id: "child", tags: [["travel", "TRAVEL"], ["zebra", "Zebra"]] },
      ] },
      { id: "escaped", tags: [["__proto__", hostile]] },
    ],
    outside: [{ id: "outside", tags: ["outside"] }],
  });
  assert.equal(app.filter.tagName, "DIV");
  assert.equal(app.filter.getAttribute("role"), "search");
  assert.equal(app.filter.getAttribute("aria-label"), "Filter tasks by tags");
  assert.equal(app.filter.querySelector("fieldset"), null);
  assert.equal(app.filter.querySelector('input[type="checkbox"]'), null);
  assert.equal(app.input.getAttribute("role"), "combobox");
  assert.equal(app.input.getAttribute("aria-label"), "Filter by tags");
  assert.equal(app.input.getAttribute("aria-autocomplete"), "list");
  assert.equal(app.input.getAttribute("placeholder"), "Filter tags…");
  assert.equal(app.input.getAttribute("type"), "text");
  assert.equal(app.input.getAttribute("aria-expanded"), "false");
  assert.equal(app.input.getAttribute("aria-activedescendant"), null);
  assert.equal(app.popup.hasAttribute("hidden"), true);
  const list = app.filter.querySelector(".ohm-todo-options");
  assert.equal(app.input.getAttribute("aria-controls"), list.id);
  assert.equal(list.getAttribute("role"), "listbox");
  assert.equal(list.getAttribute("aria-label"), "Tags");
  assert.equal(list.getAttribute("aria-multiselectable"), "true");
  assert.deepEqual(app.labels(), [hostile, "Buy", "Travel", "Zebra"]);
  for (const row of app.rows()) {
    assert.equal(row.getAttribute("role"), "option");
    assert.equal(row.closest("li.ohm-task"), null);
    assert.equal(row.querySelector(".ohm-task-tag").children.length, 0, "author labels remain text");
    assert.equal(row.getAttribute("aria-selected"), "false");
  }
  const subtitle = app.article.querySelector(".ohm-page-filename");
  assert.equal(subtitle.nextSibling, app.filter);
  const status = app.filter.querySelector(".ohm-todo-count");
  assert.equal(status.getAttribute("role"), "status");
  assert.equal(status.getAttribute("aria-live"), "polite");
  assert.equal(status.parentElement, app.filter);
  assert.equal(status.textContent, "4/4");
  assert.equal(status.getAttribute("title"), "4 of 4 tasks match");
  assert.equal(app.count(), "4 of 4 tasks match");
  assert.equal(app.tasks.get("checked").querySelector("input").checked, true);
  app.choose("travel");
  assert.deepEqual(app.visibleTasks(), ["checked", "parent", "child", "outside"]);
  assert.equal(app.count(), "2 of 4 tasks match");
  assert.equal(app.tasks.get("checked").querySelector("input").checked, true);
});

test("controls follow the own title or filename, and precede content when neither exists", () => {
  for (const [title, filename] of [[true, true], [true, false], [false, true], [false, false]]) {
    const app = fixture({ title, filename, own: [{ id: "task", tags: ["travel"] }], collected: true });
    const previous = filename ? app.article.querySelector(".ohm-page-filename") : title ? app.headings.get("todo-title") : null;
    assert.equal(previous ? previous.nextSibling : app.article.firstChild, app.filter);
    assert.equal(app.filter.parentElement, app.article);
    assert.equal(app.count(), "1 of 1 task matches");
  }
});

test("selection uses OR, excludes untagged tasks, and Clear restores all without persistence", () => {
  const options = { own: [
    { id: "travel", tags: ["travel"] },
    { id: "buy", tags: ["buy"] },
    { id: "both", tags: ["travel", "buy"] },
    { id: "other", tags: ["other"] },
    { id: "untagged" },
  ] };
  const app = fixture(options);
  assert.equal(app.count(), "5 of 5 tasks match");
  assert.equal(app.filter.querySelector(".ohm-todo-clear").disabled, true);
  app.choose("travel");
  assert.deepEqual(app.visibleTasks(), ["travel", "both"]);
  assert.equal(app.count(), "2 of 5 tasks match");
  app.choose("buy");
  assert.deepEqual(app.visibleTasks(), ["travel", "buy", "both"]);
  assert.equal(app.count(), "3 of 5 tasks match");
  app.choose("travel", false);
  assert.deepEqual(app.visibleTasks(), ["buy", "both"]);
  app.choose("buy", false);
  assert.equal(app.count(), "5 of 5 tasks match");
  app.choose("other");
  assert.equal(app.filter.querySelector(".ohm-todo-clear").disabled, false);
  app.clear();
  assert.deepEqual(app.visibleTasks(), options.own.map((task) => task.id));
  assert.equal(app.selectedTags().length, 0);
  assert.equal(app.count(), "5 of 5 tasks match");
  assert.equal(app.filter.querySelector(".ohm-todo-clear").disabled, true);
  app.choose("travel");
  assert.deepEqual(app.storageWrites, []);
  assert.equal(app.window.location.href, "https://example.test/TODO.html");
  const reloaded = fixture(options);
  assert.equal(reloaded.selectedTags().length, 0, "selection resets on reload");
  assert.equal(reloaded.input.value, "");
});

test("untagged ancestors stay for context, unrelated descendants hide, and counts exclude context", () => {
  const app = fixture({ own: [
    { id: "grandparent", children: [
      { id: "parent", tags: ["buy"], children: [
        { id: "match", tags: ["travel"] },
        { id: "sibling", tags: ["other"] },
      ] },
      { id: "uncle" },
    ] },
    { id: "separate", tags: ["other"] },
  ] });
  app.choose("travel");
  assert.deepEqual(app.visibleTasks(), ["grandparent", "parent", "match"]);
  assert.equal(app.count(), "1 of 6 tasks match");
  app.choose("travel", false);
  app.choose("buy");
  assert.deepEqual(app.visibleTasks(), ["grandparent", "parent"], "a matching parent does not select children");
  assert.equal(app.count(), "1 of 6 tasks match");
  app.choose("travel");
  assert.deepEqual(app.visibleTasks(), ["grandparent", "parent", "match"]);
  assert.equal(app.count(), "2 of 6 tasks match");
  app.clear();
  assert.equal(app.visibleTasks().length, 6);
});

test("empty generated groups, collected title and outline rows hide while authored prose remains", () => {
  const app = fixture({
    own: [{ id: "own", tags: ["home"] }],
    collected: true,
    groups: [
      { id: "one", tasks: [{ id: "one", tags: ["travel"], heading: "task-heading" }] },
      { id: "two", tasks: [{ id: "two", tags: ["buy"] }] },
    ],
  });
  assert.equal(app.observers.length, 0, "outline visibility works without IntersectionObserver");
  app.choose("travel");
  assert.equal(app.groups.get("one").hasAttribute("hidden"), false);
  assert.equal(app.groups.get("two").hasAttribute("hidden"), true);
  assert.equal(app.outlineRows.get("group-one").hasAttribute("hidden"), false);
  assert.equal(app.outlineRows.get("group-two").hasAttribute("hidden"), true);
  assert.equal(app.collected.hasAttribute("hidden"), false);
  assert.equal(app.count(), "1 of 3 tasks match");
  app.choose("travel", false);
  app.choose("home");
  assert.ok([...app.groups.values()].every((group) => group.hasAttribute("hidden")));
  assert.equal(app.collected.hasAttribute("hidden"), true);
  for (const id of ["collected-heading", "group-one", "group-two", "task-heading"]) {
    assert.equal(app.outlineRows.get(id).hasAttribute("hidden"), true, id);
  }
  assert.equal(app.prose.closest("[hidden]"), null);
  assert.equal(app.headings.get("own-heading").closest("[hidden]"), null);
  assert.equal(app.outlineRows.get("own-heading").hasAttribute("hidden"), false);
  app.clear();
  assert.equal(app.hiddenContent().length, 0);
});

test("synthetic TODO groups filter without a collected wrapper", () => {
  const app = fixture({ groups: [
    { id: "tagged", tasks: [{ id: "tagged", tags: ["travel"] }] },
    { id: "untagged", tasks: [{ id: "untagged" }] },
  ] });
  assert.equal(app.collected, undefined);
  app.choose("travel");
  assert.deepEqual(app.visibleTasks(), ["tagged"]);
  assert.equal(app.groups.get("untagged").hasAttribute("hidden"), true);
  assert.equal(app.outlineRows.get("group-untagged").hasAttribute("hidden"), true);
  assert.equal(app.count(), "1 of 2 tasks match");
  app.clear();
  assert.equal(app.hiddenContent().length, 0);
});

test("outline observation refreshes after filtering and ignores queued callbacks for old or hidden headings", () => {
  const app = fixture({ observer: true, own: [
    { id: "parent", heading: "context-heading", children: [
      { id: "match", tags: ["travel"], heading: "matching-heading" },
      { id: "hidden", tags: ["buy"], heading: "hidden-heading" },
    ] },
  ] });
  const old = app.observers.at(-1);
  const hiddenHeading = app.headings.get("hidden-heading");
  const hiddenRow = app.outlineRows.get("hidden-heading");
  const matchingHeading = app.headings.get("matching-heading");
  const matchingRow = app.outlineRows.get("matching-heading");
  old.deliver(hiddenHeading);
  assert.equal(hiddenRow.classList.contains("ohm-outline-item--active"), true);
  app.choose("travel");
  const current = app.observers.at(-1);
  assert.notEqual(current, old);
  assert.equal(old.disconnected, true);
  assert.equal(hiddenRow.hasAttribute("hidden"), true);
  assert.equal(hiddenRow.classList.contains("ohm-outline-item--active"), false);
  assert.ok(current.observed.includes(app.headings.get("context-heading")), "visible task context is observed");
  assert.ok(current.observed.includes(matchingHeading));
  assert.ok(!current.observed.includes(hiddenHeading));
  current.deliver(matchingHeading);
  assert.equal(matchingRow.classList.contains("ohm-outline-item--active"), true);
  old.deliver(hiddenHeading, app.headings.get("own-heading"));
  current.deliver(hiddenHeading);
  assert.equal(matchingRow.classList.contains("ohm-outline-item--active"), true);
  assert.equal(hiddenRow.classList.contains("ohm-outline-item--active"), false);
  app.clear();
  assert.equal(hiddenRow.hasAttribute("hidden"), false);
  assert.equal(matchingRow.classList.contains("ohm-outline-item--active"), false);
  const restored = app.observers.at(-1);
  assert.ok(restored.observed.includes(hiddenHeading));
  current.deliver(matchingHeading);
  assert.equal(matchingRow.classList.contains("ohm-outline-item--active"), false);
  restored.deliver(hiddenHeading);
  assert.equal(hiddenRow.classList.contains("ohm-outline-item--active"), true);
});

test("large tag catalogs stay bounded to eight suggestions with stable unique ids", () => {
  const tags = Array.from({ length: 32 }, (_, index) => "tag" + String(index).padStart(2, "0"));
  const app = fixture({
    own: tags.map((tag) => ({ id: tag, tags: [tag] })),
    beforeRun({ document }) {
      for (const id of ["ohm-todo-tags", "ohm-todo-tags-list-0"]) {
        const element = document.createElement("div");
        element.id = id;
        document.body.appendChild(element);
      }
    },
  });
  assert.equal(app.rows().length, 8);
  assert.deepEqual(app.labels(), tags.slice(0, 8));
  assert.equal(app.popup.hasAttribute("hidden"), true);
  const list = app.filter.querySelector(".ohm-todo-options");
  assert.equal(list.id, "ohm-todo-tags-list");
  assert.equal(app.rows()[0].id, "ohm-todo-tags-list-0-tag");
  for (const element of [list, ...app.rows()]) {
    assert.equal(app.document.querySelectorAll("#" + element.id).length, 1);
  }
  const stableId = app.rows()[5].id;
  app.open();
  assert.equal(app.input.getAttribute("aria-expanded"), "true");
  assert.equal(app.input.getAttribute("aria-activedescendant"), app.rows()[0].id);
  assert.equal(app.document.getElementById(app.input.getAttribute("aria-activedescendant")).getAttribute("role"), "option");
  assert.equal(app.filter.querySelector(".ohm-todo-hint").textContent, "Showing 8 of 32 tags. Type to narrow.");
  app.query("tag2");
  assert.deepEqual(app.labels(), tags.slice(20, 28));
  assert.equal(app.filter.querySelector(".ohm-todo-hint").textContent, "Showing 8 of 12 tags. Type to narrow.");
  app.query("tag05");
  assert.deepEqual(app.labels(), ["tag05"]);
  assert.equal(app.rows()[0].id, stableId);
  assert.equal(app.input.getAttribute("aria-activedescendant"), stableId);
  assert.equal(app.count(), "32 of 32 tasks match", "typing never filters tasks");
  app.query("zzzz");
  assert.equal(app.rows().length, 0);
  assert.equal(app.input.getAttribute("aria-activedescendant"), null);
  assert.equal(app.filter.querySelector(".ohm-todo-empty").hasAttribute("hidden"), false);
  assert.equal(app.filter.querySelector(".ohm-todo-empty").textContent, "No matching tags");
  assert.equal(app.filter.querySelector(".ohm-todo-hint").hasAttribute("hidden"), true);
  assert.equal(app.key("Enter").defaultPrevented, false);
  assert.equal(app.key("ArrowDown").defaultPrevented, true);
  assert.equal(app.input.getAttribute("aria-activedescendant"), null);
  assert.equal(app.selectedTags().length, 0);
});

test("fuzzy ranking prefers exact, prefix, substring then subsequence and ignores case and accents", () => {
  const tags = ["Travels", "Transversal", "Travelogue", "Pretravel", "Travel", "Unrelated"];
  const app = fixture({ own: tags.map((tag) => ({ id: tag, tags: [[tag.toLowerCase(), tag]] })) });
  app.query("TRÁVÉL");
  assert.deepEqual(app.labels(), ["Travel", "Travelogue", "Travels", "Pretravel", "Transversal"]);
  assert.equal(app.count(), "6 of 6 tasks match");
  app.query("trvl");
  assert.deepEqual(app.labels(), ["Pretravel", "Transversal", "Travel", "Travelogue", "Travels"]);
  app.choose("travel");
  assert.deepEqual(app.visibleTasks(), ["Travel"]);
  app.query("unrelated");
  assert.deepEqual(app.labels(), ["Unrelated"]);
  assert.deepEqual(app.visibleTasks(), ["Travel"], "query does not replace the selected filter");
  app.query("no result");
  assert.equal(app.count(), "1 of 6 tasks match");
  app.key("Escape");
  assert.equal(app.selectedTags()[0].getAttribute("aria-label"), "Remove Travel");

  const accents = fixture({ own: [
    { id: "accent", tags: [["café", "Café"]] },
    { id: "plain", tags: [["cafe", "CAFE"]] },
    { id: "prefix", tags: [["cafeteria", "Cafeteria"]] },
  ] });
  for (const query of ["cafe", "CAFÉ", "cafe\u0301"]) {
    accents.query(query);
    assert.deepEqual(accents.labels(), ["CAFE", "Café", "Cafeteria"], "equal ranks have deterministic alphabetical ties");
  }
  accents.query("cfe");
  assert.deepEqual(accents.labels(), ["CAFE", "Café", "Cafeteria"]);
});

test("keyboard navigation cycles visible options, toggles with Enter, and closes normally", () => {
  const app = fixture({ own: [
    { id: "alpha", tags: ["alpha"] },
    { id: "beta", tags: ["beta"] },
    { id: "gamma", tags: ["gamma"] },
    { id: "untagged" },
  ] });
  const activeLabel = () => app.document.getElementById(app.input.getAttribute("aria-activedescendant"))?.querySelector(".ohm-task-tag").textContent;
  app.input.focus();
  assert.equal(activeLabel(), "alpha");
  assert.equal(app.key("ArrowDown").defaultPrevented, true);
  assert.equal(activeLabel(), "beta");
  assert.equal(app.rows()[1].lastScroll.block, "nearest");
  app.key("ArrowUp");
  assert.equal(activeLabel(), "alpha");
  app.key("ArrowUp");
  assert.equal(activeLabel(), "gamma", "ArrowUp wraps to the last row");
  app.key("ArrowDown");
  assert.equal(activeLabel(), "alpha", "ArrowDown wraps to the first row");
  app.key("ArrowUp");
  assert.equal(app.key("Enter").defaultPrevented, true);
  assert.deepEqual(app.visibleTasks(), ["gamma"]);
  assert.equal(app.input.value, "");
  assert.equal(app.document.activeElement, app.input);
  assert.deepEqual(app.labels(), ["gamma", "alpha", "beta"], "empty queries show selected tags first");
  assert.equal(app.rows()[0].getAttribute("aria-selected"), "true");
  assert.equal(app.rows()[0].querySelectorAll("svg").length, 1, "selected row includes a check icon");
  app.key("ArrowDown");
  app.key("Enter");
  assert.deepEqual(app.visibleTasks(), ["alpha", "gamma"]);
  const escape = app.key("Escape");
  assert.equal(escape.defaultPrevented, true);
  assert.equal(escape.stopped, true);
  assert.equal(app.popup.hasAttribute("hidden"), true);
  assert.equal(app.input.getAttribute("aria-expanded"), "false");
  assert.equal(app.input.getAttribute("aria-activedescendant"), null);
  assert.equal(app.rows().some((row) => row.classList.contains("ohm-todo-option--active")), false);
  assert.equal(app.key("Enter").defaultPrevented, false, "closed menu has no active option to toggle");
  app.key("ArrowDown");
  assert.equal(activeLabel(), "alpha", "ArrowDown reopens at the first option");
  app.key("Escape");
  app.key("ArrowUp");
  assert.equal(activeLabel(), "beta", "ArrowUp reopens at the last option");
  assert.equal(app.key("Tab").defaultPrevented, false);
  assert.equal(app.popup.hasAttribute("hidden"), true);
  assert.equal(app.count(), "2 of 4 tasks match");
  app.open();
  assert.equal(app.input.getAttribute("aria-expanded"), "true", "clicking the still-focused input reopens it");
});

test("IME composition never toggles a suggestion until composition finishes", () => {
  const app = fixture({ own: [{ id: "travel", tags: ["travel"] }, { id: "other" }] });
  app.query("travel");
  assert.equal(app.key("Enter", { isComposing: true }).defaultPrevented, false);
  assert.equal(app.key("Enter", { keyCode: 229 }).defaultPrevented, false);
  app.input.dispatchEvent({ type: "compositionstart" });
  assert.equal(app.key("Enter").defaultPrevented, false);
  assert.equal(app.key("Escape").defaultPrevented, false);
  assert.equal(app.selectedTags().length, 0);
  assert.equal(app.count(), "2 of 2 tasks match");
  app.input.dispatchEvent({ type: "compositionend" });
  assert.equal(app.key("Enter").defaultPrevented, true);
  assert.equal(app.selectedTags().length, 1);
  assert.equal(app.count(), "1 of 2 tasks match");
});

test("pointer picking, chip removal and Clear retain focus, while outside pointer and focus dismiss", () => {
  const app = fixture({ own: [{ id: "buy", tags: ["buy"] }, { id: "travel", tags: ["travel"] }] });
  app.open();
  const row = app.rows()[1];
  const pointer = { type: "pointerdown" };
  row.dispatchEvent(pointer);
  assert.equal(pointer.defaultPrevented, true, "pointer picking preserves input focus");
  assert.equal(app.document.activeElement, app.input);
  row.dispatchEvent({ type: "pointermove" });
  assert.equal(app.input.getAttribute("aria-activedescendant"), row.id);
  row.querySelector(".ohm-task-tag").dispatchEvent({ type: "click" });
  assert.deepEqual(app.visibleTasks(), ["travel"]);
  assert.equal(app.document.activeElement, app.input);
  const chip = app.selectedTags()[0];
  chip.focus();
  assert.equal(app.popup.hasAttribute("hidden"), false, "focus within the picker keeps it open");
  chip.dispatchEvent({ type: "click" });
  assert.equal(app.selectedTags().length, 0);
  assert.equal(app.count(), "2 of 2 tasks match");
  assert.equal(app.document.activeElement, app.input);
  app.query("trvl");
  assert.equal(app.filter.querySelector(".ohm-todo-clear").disabled, false, "query alone can be cleared");
  app.clear();
  assert.equal(app.input.value, "");
  assert.equal(app.filter.querySelector(".ohm-todo-clear").disabled, true);
  assert.equal(app.document.activeElement, app.input);
  assert.equal(app.filter.querySelector(".ohm-todo-clear").getAttribute("aria-label"), "Clear tag filter");
  app.prose.dispatchEvent({ type: "pointerdown" });
  assert.equal(app.popup.hasAttribute("hidden"), true);
  app.open();
  const outside = app.document.createElement("button");
  app.document.body.appendChild(outside);
  outside.focus();
  assert.equal(app.popup.hasAttribute("hidden"), true);
  assert.equal(app.input.getAttribute("aria-activedescendant"), null);
  app.input.focus();
  assert.equal(app.popup.hasAttribute("hidden"), false);
  app.input.dispatchEvent({ type: "focusout", relatedTarget: null });
  assert.equal(app.popup.hasAttribute("hidden"), true, "leaving the document closes the menu too");
});

test("selected tags show at most two chips plus a compact overflow control and remain individually toggleable", () => {
  const tags = Array.from({ length: 12 }, (_, index) => "tag" + String(index).padStart(2, "0"));
  const app = fixture({ own: [...tags.map((tag) => ({ id: tag, tags: [tag] })), { id: "untagged" }] });
  for (const key of ["tag09", "tag03", "tag11", "tag05"]) app.choose(key);
  assert.equal(app.selectedTags().length, 2);
  assert.deepEqual(app.selectedTags().map((tag) => tag.textContent), ["tag09", "tag03"]);
  const more = app.filter.querySelector(".ohm-todo-more");
  assert.equal(more.textContent, "+2");
  assert.equal(more.getAttribute("title"), "tag11, tag05");
  assert.equal(app.count(), "4 of 13 tasks match");
  app.key("Escape");
  more.dispatchEvent({ type: "click" });
  assert.equal(app.popup.hasAttribute("hidden"), false);
  assert.equal(app.document.activeElement, app.input);
  assert.deepEqual(app.labels().slice(0, 4), ["tag03", "tag05", "tag09", "tag11"]);
  app.query("tag11");
  app.key("Enter");
  assert.equal(app.count(), "3 of 13 tasks match");
  assert.equal(app.filter.querySelector(".ohm-todo-more").textContent, "+1");
  app.selectedTags()[0].dispatchEvent({ type: "click" });
  assert.deepEqual(app.selectedTags().map((tag) => tag.textContent), ["tag03", "tag05"]);
  assert.equal(app.filter.querySelector(".ohm-todo-more"), null);
  assert.equal(app.count(), "2 of 13 tasks match");
  app.clear();
  assert.equal(app.selectedTags().length, 0);
  assert.equal(app.filter.querySelector(".ohm-todo-more"), null);
  assert.equal(app.count(), "13 of 13 tasks match");
});

test("suggestions and selected chips copy valid source colors while unsafe labels stay text", () => {
  const hostile = '<img src=x onerror="alert(1)">';
  const own = Array.from({ length: 8 }, (_, color) => ({ id: "color" + color, tags: [["color" + color, "Color " + color, String(color)]] }));
  own.push(
    { id: "duplicate", tags: [["color3", "COLOR 3", "6"]] },
    { id: "neutral", tags: [["neutral", "Neutral", "8"]] },
    { id: "missing", tags: [["missing", "Missing"]] },
    { id: "unsafe", tags: [["__proto__", hostile, "0; color: red"]] },
  );
  const app = fixture({ own });
  for (let color = 0; color < 8; color++) {
    app.query("color " + color);
    assert.equal(app.rows()[0].querySelector(".ohm-task-tag").getAttribute("data-ohm-tag-color"), String(color));
  }
  app.choose("color3");
  assert.equal(app.selectedTags()[0].getAttribute("data-ohm-tag-color"), "3", "first badge supplies the catalog color");
  for (const key of ["neutral", "missing", "__proto__"]) {
    app.choose(key);
    app.query(key === "__proto__" ? hostile : key);
    assert.equal(app.rows()[0].querySelector(".ohm-task-tag").getAttribute("data-ohm-tag-color"), null);
  }
  app.clear();
  app.choose("__proto__");
  const chip = app.selectedTags()[0];
  assert.equal(chip.getAttribute("data-ohm-tag-color"), null);
  assert.equal(chip.getAttribute("aria-label"), "Remove " + hostile);
  assert.equal(chip.querySelector("span").textContent, hostile);
  assert.equal(chip.querySelector("span").children.length, 0);
  app.query(hostile);
  assert.equal(app.rows()[0].querySelector(".ohm-task-tag").children.length, 0);
  assert.equal(app.rows()[0].getAttribute("aria-selected"), "true");
});

const sortableGroups = () => [
  { id: "first-file", title: "Zulu 10", created: "20", updated: "10", tasks: [{ id: "first-task" }] },
  { id: "second-file", title: "alpha 10", created: "10", updated: "30", tasks: [{ id: "second-task" }] },
  { id: "third-file", title: "Alpha 2", created: "30", updated: "20", tasks: [{ id: "third-task" }] },
  { id: "fourth-file", title: "alpha 2", created: "30", updated: "20", tasks: [{ id: "fourth-task" }] },
  { id: "unknown-file", title: "Beta", tasks: [{ id: "unknown-task" }] },
];

test("TODO sorting has the sidebar's seven native modes and restores initial filename order", () => {
  const groups = sortableGroups();
  const expected = new Map([
    ["default", groups.map((group) => group.id)],
    ["name-asc", ["third-file", "fourth-file", "second-file", "unknown-file", "first-file"]],
    ["name-desc", ["first-file", "unknown-file", "second-file", "third-file", "fourth-file"]],
    ["created-desc", ["third-file", "fourth-file", "first-file", "second-file", "unknown-file"]],
    ["created-asc", ["second-file", "first-file", "third-file", "fourth-file", "unknown-file"]],
    ["updated-desc", ["second-file", "third-file", "fourth-file", "first-file", "unknown-file"]],
    ["updated-asc", ["first-file", "third-file", "fourth-file", "second-file", "unknown-file"]],
  ]);
  const labels = ["Default (filename order)", "Name A–Z", "Name Z–A", "Created newest", "Created oldest", "Updated newest", "Updated oldest"];
  for (const collected of [false, true]) {
    const app = fixture({ groups, collected });
    assert.equal(app.filter, null, "sorting does not create a tag-filter toolbar without tags");
    const select = app.sortSelect;
    const control = select.parentElement;
    assert.equal(control.tagName, "LABEL");
    for (const className of ["ohm-icon-button", "ohm-nav-sort", "ohm-todo-sort"]) {
      assert.equal(control.classList.contains(className), true);
    }
    assert.match(control.innerHTML, /<svg class="ohm-icon"/);
    assert.equal(select.tagName, "SELECT");
    assert.equal(select.classList.contains("ohm-nav-sort-select"), true);
    assert.equal(select.getAttribute("aria-label"), "Sort TODO files");
    assert.deepEqual(select.children.map((option) => option.value), [...expected.keys()]);
    assert.deepEqual(select.children.map((option) => option.textContent), labels);
    assert.deepEqual(app.storageWrites, [], "a clean first visit is unwritten");
    for (const [mode, order] of expected) {
      app.sort(mode);
      assert.deepEqual(app.groupOrder(), order, mode);
      assert.equal(select.value, mode);
      assert.equal(control.getAttribute("data-ohm-sort-mode"), mode);
      assert.equal(control.classList.contains("ohm-nav-sort--active"), mode !== "default");
      const title = "Sort TODO files: " + labels[[...expected.keys()].indexOf(mode)];
      assert.equal(control.getAttribute("title"), title);
      assert.equal(select.getAttribute("title"), title);
    }
    app.sort("default");
    assert.deepEqual(app.groupOrder(), groups.map((group) => group.id));
    assert.equal(app.storage.entries.has(sortKey()), false);
    select.focus();
    for (const [type, key] of [["pointerdown"], ["click"], ["keydown", "ArrowDown"], ["keyup", "ArrowDown"]]) {
      const event = { type, key };
      assert.equal(select.dispatchEvent(event), true, "native " + type + " behavior remains available");
      assert.equal(event.defaultPrevented, false);
    }
    assert.equal(app.document.activeElement, select);
  }
});

test("TODO date sorting accepts safe signed milliseconds and puts unknown and invalid dates last", () => {
  const invalid = [undefined, null, "", " ", "10.5", "1e3", "+1", "NaN", "Infinity", "9007199254740992", "-9007199254740992"];
  const groups = [
    { id: "zero", title: "Zero", created: "0", updated: "0", tasks: [] },
    ...invalid.map((value, index) => ({ id: "invalid" + index, title: "Unknown " + (invalid.length - index), created: value, updated: value, tasks: [] })),
    { id: "maximum", title: "Maximum", created: "9007199254740991", updated: "9007199254740991", tasks: [] },
    { id: "negative", title: "Negative", created: "-1", updated: "-1", tasks: [] },
    { id: "minimum", title: "Minimum", created: "-9007199254740991", updated: "-9007199254740991", tasks: [] },
    { id: "zero-alpha10", title: "alpha 10", created: "0", updated: "0", tasks: [] },
    { id: "zero-alpha2", title: "Alpha 2", created: "-0", updated: "-0", tasks: [] },
    { id: "zero-alpha2-tie", title: "alpha 2", created: "0", updated: "0", tasks: [] },
  ];
  const app = fixture({ groups });
  const unknown = invalid.map((_, index) => "invalid" + (invalid.length - index - 1));
  const zeros = ["zero-alpha2", "zero-alpha2-tie", "zero-alpha10", "zero"];
  for (const field of ["created", "updated"]) {
    app.sort(field + "-asc");
    assert.deepEqual(app.groupOrder(), ["minimum", "negative", ...zeros, "maximum", ...unknown]);
    app.sort(field + "-desc");
    assert.deepEqual(app.groupOrder(), ["maximum", ...zeros, "negative", "minimum", ...unknown]);
  }
});

test("TODO sort controls require two generated groups and stay outside collected content", () => {
  for (const options of [
    { scoped: false, groups: sortableGroups() },
    { groups: [] },
    { groups: sortableGroups().slice(0, 1) },
    { own: [{ id: "own", tags: ["travel"] }], groups: [] },
    { own: [{ id: "own", tags: ["travel"] }], groups: sortableGroups().slice(0, 1) },
  ]) {
    const app = fixture(options);
    assert.equal(app.sortSelect, null);
    assert.equal(app.document.querySelector(".ohm-todo-controls"), null);
    assert.deepEqual(app.storage.calls, [], "irrelevant pages do not access sorting preferences");
  }
  for (const [title, filename] of [[true, true], [true, false], [false, true], [false, false]]) {
    const app = fixture({ title, filename, collected: true, groups: sortableGroups() });
    const toolbar = app.document.querySelector(".ohm-todo-controls");
    const previous = filename ? app.article.querySelector(".ohm-page-filename") : title ? app.headings.get("todo-title") : null;
    assert.equal(previous ? previous.nextSibling : app.article.firstChild, toolbar);
    assert.equal(toolbar.parentElement, app.article);
    assert.equal(app.sortSelect.closest("[data-ohm-todo-collected]"), null);
    assert.equal(app.filter, null);
  }
  const tagged = fixture({ own: [{ id: "own", tags: ["home"] }], groups: sortableGroups(), collected: true });
  assert.equal(tagged.sortSelect.parentElement.parentElement, tagged.filter);
  assert.equal(tagged.filter.children.at(-1), tagged.sortSelect.parentElement);
  assert.equal(tagged.document.querySelector(".ohm-todo-controls"), null);
  tagged.choose("home");
  assert.equal(tagged.collected.hasAttribute("hidden"), true);
  assert.equal(tagged.sortSelect.closest("[hidden]"), null, "sorting stays accessible when every file is filtered out");
});

test("TODO sorting preserves authored slots, section identity, listeners and within-file task order", () => {
  for (const collected of [false, true]) {
    let separator;
    let last;
    const app = fixture({
      collected,
      own: [{ id: "own", children: [{ id: "own-child" }] }],
      groups: [
        { id: "z", title: "Zulu", tasks: [{ id: "z-first", children: [{ id: "z-child" }] }, { id: "z-last" }] },
        { id: "a", title: "Alpha", tasks: [{ id: "a-first" }, { id: "a-last" }] },
      ],
      beforeRun({ document, article, collected: wrapper, groups }) {
        const parent = wrapper || article;
        separator = document.createElement("p");
        separator.textContent = "Fixed content between files";
        parent.insertBefore(separator, groups.get("a"));
        last = document.createElement("p");
        last.textContent = "Fixed trailing content";
        parent.appendChild(last);
      },
    });
    const parent = app.collected || app.article;
    const originalSlots = [...parent.children];
    const authored = new Map([...app.article.children].map((node, index) => [node, index])
      .filter(([node]) => !node.hasAttribute("data-ohm-todo-group")));
    const sections = new Map(app.groups);
    const tasks = new Map([...app.groups].map(([id, section]) => [id, section.querySelectorAll("li.ohm-task")]));
    let clicks = 0;
    app.tasks.get("z-first").addEventListener("click", () => { clicks++; });
    app.sort("name-asc");
    assert.deepEqual(app.groupOrder(), ["a", "z"]);
    assert.equal(parent.children.indexOf(separator), originalSlots.indexOf(separator));
    assert.equal(parent.children.indexOf(last), originalSlots.indexOf(last));
    for (const [node, index] of authored) assert.equal(app.article.children.indexOf(node), index);
    for (const [id, section] of sections) {
      assert.equal(app.groups.get(id), section);
      assert.deepEqual(section.querySelectorAll("li.ohm-task"), tasks.get(id));
    }
    app.tasks.get("z-first").dispatchEvent({ type: "click" });
    assert.equal(clicks, 1);
    app.sort("default");
    assert.deepEqual(parent.children, originalSlots);
  }
});

test("TODO sorting moves complete flat outline blocks and refreshes observers with active filters", () => {
  const app = fixture({
    collected: true,
    observer: true,
    own: [{ id: "own", tags: ["home"], heading: "own-task-heading" }],
    groups: [
      { id: "z", title: "Zulu", tasks: [{ id: "z", tags: ["travel"], heading: "z-heading", children: [{ id: "nested", tags: ["buy"], heading: "nested-heading" }] }] },
      { id: "a", title: "Alpha", tasks: [{ id: "a", tags: ["buy"], heading: "a-heading" }] },
      { id: "b", title: "Beta", tasks: [{ id: "b", tags: ["travel"], heading: "b-heading" }] },
    ],
  });
  const authored = ["todo-title", "own-heading", "own-task-heading", "collected-heading"];
  const originalOutline = app.outlineOrder();
  const rows = new Map(app.outlineRows);
  app.choose("travel");
  const beforeSort = app.observers.at(-1);
  beforeSort.deliver(app.headings.get("z-heading"));
  assert.equal(app.outlineRows.get("z-heading").classList.contains("ohm-outline-item--active"), true);
  app.sort("name-asc");
  assert.deepEqual(app.groupOrder(), ["a", "b", "z"], "hidden files move too");
  assert.deepEqual(app.outlineOrder(), [...authored, "group-a", "a-heading", "group-b", "b-heading", "group-z", "z-heading", "nested-heading"]);
  assert.equal(beforeSort.disconnected, true);
  const sortedObserver = app.observers.at(-1);
  assert.notEqual(sortedObserver, beforeSort);
  assert.deepEqual(sortedObserver.observed.map((heading) => heading.id),
    ["todo-title", "own-heading", "collected-heading", "group-b", "b-heading", "group-z", "z-heading"]);
  for (const id of ["group-a", "a-heading", "nested-heading", "own-task-heading"]) {
    assert.equal(app.outlineRows.get(id).hasAttribute("hidden"), true, id);
    assert.equal(sortedObserver.observed.includes(app.headings.get(id)), false, id);
  }
  for (const id of ["collected-heading", "group-b", "b-heading", "group-z", "z-heading"]) {
    assert.equal(app.outlineRows.get(id).hasAttribute("hidden"), false, id);
    assert.equal(sortedObserver.observed.includes(app.headings.get(id)), true, id);
  }
  beforeSort.deliver(app.headings.get("z-heading"));
  assert.equal(app.outlineRows.get("z-heading").classList.contains("ohm-outline-item--active"), false, "old observer callbacks cannot reactivate a moved row");
  sortedObserver.deliver(app.headings.get("b-heading"));
  assert.equal(app.outlineRows.get("b-heading").classList.contains("ohm-outline-item--active"), true);
  app.clear();
  assert.deepEqual(app.groupOrder(), ["a", "b", "z"], "clearing a filter preserves sorting");
  assert.deepEqual(app.outlineOrder(), [...authored, "group-a", "a-heading", "group-b", "b-heading", "group-z", "z-heading", "nested-heading"]);
  assert.equal(app.hiddenContent().length, 0);
  assert.equal(app.sortSelect.value, "name-asc");
  assert.equal(app.observers.at(-1).observed.includes(app.headings.get("nested-heading")), true);
  app.sort("default");
  assert.deepEqual(app.outlineOrder(), originalOutline);
  for (const [id, row] of rows) assert.equal(app.outlineRows.get(id), row, "outline node identity: " + id);
});

test("TODO sort selection persists for the resolved site across navigation and stays independent", () => {
  const sidebarKey = "ohimark-sidebar-sort:" + SITE;
  const collapseKey = "ohimark-sidebar-groups:" + SITE;
  const storage = new Storage([[sidebarKey, '{"":"updated-asc"}'], [collapseKey, '["notes"]']]);
  const options = { groups: sortableGroups(), own: [{ id: "own", tags: ["home"] }], storage };
  const first = fixture(options);
  first.choose("home");
  assert.deepEqual(storage.writes, [], "filtering and the first visit do not write a sorting preference");
  first.sort("created-desc");
  assert.equal(storage.entries.get(sortKey()), "created-desc");
  assert.deepEqual(storage.writes, [["set", sortKey(), "created-desc"]]);
  const navigated = fixture({ ...options, site: "../index.html", url: "https://example.test/docs/deep/TODO.html" });
  assert.equal(navigated.sortSelect.value, "created-desc");
  assert.deepEqual(navigated.groupOrder(), ["third-file", "fourth-file", "first-file", "second-file", "unknown-file"]);
  assert.equal(navigated.selectedTags().length, 0, "tag filtering resets independently on navigation");
  assert.equal(storage.writes.length, 1, "restoring a valid preference does not rewrite it");
  const otherSite = "https://example.test/other/index.html";
  const other = fixture({ ...options, site: otherSite });
  assert.equal(other.sortSelect.value, "default");
  other.sort("name-desc");
  assert.equal(storage.entries.get(sortKey(otherSite)), "name-desc");
  assert.equal(storage.entries.get(sortKey()), "created-desc");
  first.sort("default");
  assert.equal(storage.entries.has(sortKey()), false);
  assert.equal(storage.entries.get(sortKey(otherSite)), "name-desc");
  assert.equal(storage.entries.get(sidebarKey), '{"":"updated-asc"}');
  assert.equal(storage.entries.get(collapseKey), '["notes"]');
  assert.equal(fixture(options).sortSelect.value, "default");
  const freshSession = fixture({ groups: sortableGroups() });
  assert.equal(freshSession.sortSelect.value, "default");
  assert.deepEqual(freshSession.storageWrites, []);
});

test("TODO sorting cleans invalid preferences and tolerates missing brand and blocked storage", () => {
  for (const stored of ["default", "", "unknown-mode", '"name-asc"', "{}", "[]", "null", "{broken"]) {
    const storage = new Storage([[sortKey(), stored]]);
    const app = fixture({ groups: sortableGroups(), storage });
    assert.equal(app.sortSelect.value, "default", stored);
    assert.deepEqual(app.groupOrder(), sortableGroups().map((group) => group.id));
    assert.equal(storage.entries.has(sortKey()), false);
    assert.deepEqual(storage.writes, [["remove", sortKey()]]);
  }
  const noBrand = fixture({ site: null, groups: sortableGroups() });
  noBrand.sort("name-asc");
  assert.equal(noBrand.groupOrder()[0], "third-file");
  assert.deepEqual(noBrand.storage.calls, [], "without a site brand sorting skips persistence");
  const blockedRead = new Storage([[sortKey(), "name-asc"]]);
  blockedRead.block.add("get");
  const unreadable = fixture({ groups: sortableGroups(), storage: blockedRead });
  assert.equal(unreadable.sortSelect.value, "default");
  unreadable.sort("name-desc");
  assert.equal(unreadable.groupOrder()[0], "first-file");
  assert.equal(blockedRead.entries.get(sortKey()), "name-desc");
  const blockedWrite = new Storage();
  blockedWrite.block.add("set");
  const unwritable = fixture({ groups: sortableGroups(), storage: blockedWrite });
  unwritable.sort("name-asc");
  assert.equal(unwritable.groupOrder()[0], "third-file");
  assert.equal(blockedWrite.entries.has(sortKey()), false);
  const blockedRemoval = new Storage([[sortKey(), "created-desc"]]);
  blockedRemoval.block.add("remove");
  const unremovable = fixture({ groups: sortableGroups(), storage: blockedRemoval });
  unremovable.sort("default");
  assert.deepEqual(unremovable.groupOrder(), sortableGroups().map((group) => group.id));
  assert.equal(blockedRemoval.entries.get(sortKey()), "created-desc");
  const inaccessible = fixture({
    groups: sortableGroups(),
    beforeRun({ window }) {
      Object.defineProperty(window, "sessionStorage", { get() { throw new Error("Storage inaccessible"); } });
    },
  });
  inaccessible.sort("updated-asc");
  assert.equal(inaccessible.groupOrder()[0], "first-file");
});
