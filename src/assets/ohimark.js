/*
 * ohimark.js — progressive-enhancement script for every ohimark-built site.
 *
 * Implements: R8.1 (theme toggle button, kept in sync with the inline head
 * script that already applied the stored/system theme before paint), R8.6
 * (per-diagram source/graphic toggle), R8.10 (copy-to-clipboard on code
 * blocks and diagrams), plus TODO tag filtering, outline scroll-highlighting
 * and collapsible sidebars with a mobile navigation drawer.
 *
 * Loaded with `defer`. Purely additive: the page is already correct and
 * fully usable (R8.8) before this file runs, so every DOM query is guarded
 * and every failure is swallowed rather than left to break the page. All
 * icons are inline SVG (owner request: no undersized text glyphs), drawn
 * from one small stroke-based icon set matching the panel controls that
 * theme.rs already emits server-side.
 */

(function () {
  "use strict";

  var ICONS = {
    sun: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"/></svg>',
    moon: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M20 14.5A8.5 8.5 0 1 1 9.5 4a7 7 0 0 0 10.5 10.5Z"/></svg>',
    clipboard: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><rect x="8" y="2" width="8" height="4" rx="1"/><path d="M8 4H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2h-2"/></svg>',
    check: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M20 6 9 17l-5-5"/></svg>',
    code: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="m9 8-5 4 5 4M15 8l5 4-5 4"/></svg>',
    sort: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M4 6h16M4 12h10M4 18h4M17 12v8m-3-3 3 3 3-3"/></svg>',
    reset: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M3 11a9 9 0 1 1 2.6 7M3 4v7h7"/></svg>',
    chevronLeft: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="m14 6-6 6 6 6"/></svg>',
    fullscreen: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M8 3H5a2 2 0 0 0-2 2v3M16 3h3a2 2 0 0 1 2 2v3M21 16v3a2 2 0 0 1-2 2h-3M8 21H5a2 2 0 0 1-2-2v-3"/></svg>',
    fullscreenExit: '<svg class="ohm-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="M8 3v3a2 2 0 0 1-2 2H3M16 3v3a2 2 0 0 0 2 2h3M21 16h-3a2 2 0 0 0-2 2v3M8 21v-3a2 2 0 0 0-2-2H3"/></svg>',
  };

  /* =======================================================================
     1. Theme toggle (R8.1)
     ======================================================================= */

  function readStoredTheme() {
    try {
      return window.sessionStorage.getItem("ohimark-theme");
    } catch (err) {
      /* sessionStorage can throw in private windows or over some file://
         origins; fall back to "no stored preference". */
      return null;
    }
  }

  function writeStoredTheme(value) {
    try {
      window.sessionStorage.setItem("ohimark-theme", value);
    } catch (err) {
      /* Ignore: persistence is best-effort only. */
    }
  }

  function systemPrefersDark() {
    return !!(
      window.matchMedia &&
      window.matchMedia("(prefers-color-scheme: dark)").matches
    );
  }

  function currentTheme() {
    var stored = readStoredTheme();
    if (stored === "light" || stored === "dark") {
      return stored;
    }
    var attr = document.documentElement.getAttribute("data-theme");
    if (attr === "light" || attr === "dark") {
      return attr;
    }
    return systemPrefersDark() ? "dark" : "light";
  }

  var themeButton = document.getElementById("ohm-theme-button");

  function syncThemeButton(theme) {
    if (!themeButton) return;
    var isDark = theme === "dark";
    themeButton.innerHTML = isDark ? ICONS.sun : ICONS.moon;
    themeButton.setAttribute("aria-pressed", isDark ? "true" : "false");
    themeButton.setAttribute("aria-label", isDark ? "Switch to light theme" : "Switch to dark theme");
  }

  function applyTheme(theme) {
    document.documentElement.setAttribute("data-theme", theme);
    writeStoredTheme(theme);
    syncThemeButton(theme);
  }

  if (themeButton) {
    // Only reveal the control once script has run, per R8.8: a theme
    // toggle that does nothing without JS must never appear.
    themeButton.removeAttribute("hidden");
    syncThemeButton(currentTheme());
  }

  /* =======================================================================
     2. Diagram controls: copy source + graphic/source toggle (R8.6, R8.10)
     ======================================================================= */

  document.querySelectorAll(".ohm-diagram").forEach(function (figure) {
    var graphic = figure.querySelector(".ohm-diagram-graphic");
    var source = figure.querySelector(".ohm-diagram-source");
    if (!graphic || !source) return;

    var actions = document.createElement("div");
    actions.className = "ohm-diagram-actions";

    var copyButton = document.createElement("button");
    copyButton.type = "button";
    copyButton.className = "ohm-icon-button ohm-copy-button";
    copyButton.setAttribute("aria-label", "Copy diagram source");
    copyButton.innerHTML = ICONS.clipboard;

    var toggleButton = document.createElement("button");
    toggleButton.type = "button";
    toggleButton.className = "ohm-icon-button ohm-diagram-toggle";
    toggleButton.setAttribute("aria-expanded", "false");
    toggleButton.setAttribute("aria-label", "Show diagram source");
    toggleButton.innerHTML = ICONS.code;

    actions.appendChild(copyButton);
    actions.appendChild(toggleButton);

    if (
      document.fullscreenEnabled &&
      typeof document.exitFullscreen === "function" &&
      typeof figure.requestFullscreen === "function"
    ) {
      var fullscreenButton = document.createElement("button");
      fullscreenButton.type = "button";
      fullscreenButton.className = "ohm-icon-button ohm-diagram-fullscreen";
      fullscreenButton.setAttribute("aria-pressed", "false");
      fullscreenButton.setAttribute("aria-label", "Enter diagram fullscreen");
      fullscreenButton.setAttribute("title", "Enter diagram fullscreen");
      fullscreenButton.innerHTML = ICONS.fullscreen;
      actions.appendChild(fullscreenButton);
    }

    figure.appendChild(actions);
  });

  function toggleDiagram(button) {
    var figure = button.closest(".ohm-diagram");
    if (!figure) return;
    var showingSource = figure.classList.toggle("ohm-diagram--source");
    button.setAttribute("aria-expanded", showingSource ? "true" : "false");
    button.setAttribute("aria-label", showingSource ? "Show diagram" : "Show diagram source");
  }

  function syncDiagramFullscreenButtons() {
    document.querySelectorAll(".ohm-diagram-fullscreen").forEach(function (button) {
      var figure = button.closest(".ohm-diagram");
      var isFullscreen = !!figure && document.fullscreenElement === figure;
      var label = isFullscreen ? "Exit diagram fullscreen" : "Enter diagram fullscreen";
      button.innerHTML = isFullscreen ? ICONS.fullscreenExit : ICONS.fullscreen;
      button.setAttribute("aria-pressed", isFullscreen ? "true" : "false");
      button.setAttribute("aria-label", label);
      button.setAttribute("title", label);
    });
  }

  function syncAfterFullscreenChange(result) {
    if (result && typeof result.then === "function") {
      result.then(syncDiagramFullscreenButtons, syncDiagramFullscreenButtons);
      return;
    }
    syncDiagramFullscreenButtons();
  }

  function toggleDiagramFullscreen(button) {
    var figure = button.closest(".ohm-diagram");
    if (!figure) return;

    try {
      if (document.fullscreenElement === figure) {
        syncAfterFullscreenChange(document.exitFullscreen());
      } else {
        syncAfterFullscreenChange(figure.requestFullscreen());
      }
    } catch (err) {
      syncDiagramFullscreenButtons();
    }
  }

  document.addEventListener("fullscreenchange", syncDiagramFullscreenButtons);
  syncDiagramFullscreenButtons();

  /* =======================================================================
     3. Code-block copy buttons (R8.10)
     Injected as a sibling of <code>, inside the already-positioned <pre>,
     so the button is never part of the copied text. Excludes diagram
     source blocks, which already get their own copy button above.
     ======================================================================= */

  document.querySelectorAll(".ohm-content pre:not(.ohm-diagram-source)").forEach(function (pre) {
    if (!pre.querySelector("code")) return;
    var button = document.createElement("button");
    button.type = "button";
    button.className = "ohm-icon-button ohm-copy-button";
    button.setAttribute("aria-label", "Copy code");
    button.innerHTML = ICONS.clipboard;
    pre.appendChild(button);
  });

  /* -- Shared copy-to-clipboard handling, with a file://-safe fallback -- */

  var liveRegion = document.createElement("div");
  liveRegion.className = "ohm-visually-hidden";
  liveRegion.setAttribute("aria-live", "polite");
  document.body.appendChild(liveRegion);

  function announce(message) {
    liveRegion.textContent = message;
  }

  function legacyCopy(text) {
    try {
      var textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.setAttribute("readonly", "");
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      var ok = document.execCommand("copy");
      document.body.removeChild(textarea);
      return ok;
    } catch (err) {
      return false;
    }
  }

  function attemptClipboard(text) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      return navigator.clipboard.writeText(text).catch(function () {
        return legacyCopy(text) ? Promise.resolve() : Promise.reject();
      });
    }
    return legacyCopy(text) ? Promise.resolve() : Promise.reject();
  }

  function onCopyResult(button, ok) {
    var originalIcon = button.innerHTML;
    var originalLabel = button.getAttribute("aria-label");
    if (ok) {
      button.innerHTML = ICONS.check;
      button.setAttribute("aria-label", "Copied");
      announce("Copied to clipboard");
    } else {
      button.setAttribute("aria-label", "Copy failed");
      announce("Copy failed");
    }
    setTimeout(function () {
      button.innerHTML = originalIcon;
      button.setAttribute("aria-label", originalLabel);
    }, 1500);
  }

  function handleCopy(button) {
    var figure = button.closest(".ohm-diagram");
    var codeEl = figure
      ? figure.querySelector(".ohm-diagram-source code")
      : button.parentElement.querySelector("code");
    if (!codeEl) return;
    attemptClipboard(codeEl.textContent).then(
      function () { onCopyResult(button, true); },
      function () { onCopyResult(button, false); }
    );
  }

  /* =======================================================================
     4. Outline scroll-highlighting (decorative only)
     ======================================================================= */

  var outlineLinks = document.querySelectorAll('.ohm-outline-item > a[href^="#"]');
  var outlineHeadings = [];
  var itemByHeadingId = new Map();
  var outlineObserver = null;
  var outlineGeneration = 0;

  outlineLinks.forEach(function (link) {
    var id = (link.getAttribute("href") || "").slice(1);
    var heading = id && document.getElementById(id);
    if (!heading) return;
    itemByHeadingId.set(id, link.parentElement);
    outlineHeadings.push(heading);
  });

  function setHidden(element, hidden) {
    if (hidden) element.setAttribute("hidden", "");
    else element.removeAttribute("hidden");
  }

  function setActiveOutlineItem(item) {
    var current = document.querySelector(".ohm-outline-item--active");
    if (current && current !== item) {
      current.classList.remove("ohm-outline-item--active");
    }
    if (item) item.classList.add("ohm-outline-item--active");
  }

  function refreshOutline() {
    if (outlineObserver) outlineObserver.disconnect();
    var generation = ++outlineGeneration;
    var visibleHeadings = outlineHeadings.filter(function (heading) {
      var item = itemByHeadingId.get(heading.id);
      var hidden = !!heading.closest("[hidden]");
      setHidden(item, hidden);
      item.classList.remove("ohm-outline-item--active");
      return !hidden;
    });
    if (!visibleHeadings.length || typeof window.IntersectionObserver !== "function") return;

    outlineObserver = new window.IntersectionObserver(
      function (entries) {
        // A disconnected observer can still deliver queued entries after a
        // filter change. Neither those entries nor hidden headings are active.
        if (generation !== outlineGeneration) return;
        for (var k = 0; k < entries.length; k++) {
          var entry = entries[k];
          if (!entry.isIntersecting || entry.target.closest("[hidden]")) continue;
          var item = itemByHeadingId.get(entry.target.id);
          if (item && !item.closest("[hidden]")) setActiveOutlineItem(item);
        }
      },
      { rootMargin: "0px 0px -70% 0px", threshold: 0 }
    );
    visibleHeadings.forEach(function (heading) { outlineObserver.observe(heading); });
  }

  refreshOutline();

  /* =======================================================================
     5. TODO tag filters. The complete list and badges are usable without JS;
     controls and selection exist only for this visit to the final TODO page.
     ======================================================================= */

  function enhanceTodoFiltering() {
    var scope = document.querySelector("article[data-ohm-todo]");
    if (!scope) return;

    function foldTag(value) {
      return value.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
    }

    var catalog = new Map();
    var tasks = Array.from(scope.querySelectorAll("li.ohm-task")).map(function (element) {
      var keys = [];
      try {
        var parsed = JSON.parse(element.getAttribute("data-ohm-tags"));
        if (Array.isArray(parsed)) keys = parsed;
      } catch (err) {
        /* Invalid author-supplied metadata behaves like an untagged task. */
      }
      var labels = Array.from(element.querySelectorAll(".ohm-task-tags > .ohm-task-tag")).filter(function (label) {
        return label.closest("li.ohm-task") === element;
      });
      var tags = new Set();
      keys.forEach(function (key, index) {
        if (typeof key !== "string" || !key) return;
        key = key.toLowerCase();
        tags.add(key);
        if (!catalog.has(key)) {
          var label = labels[index] ? labels[index].textContent : key;
          var color = labels[index] && labels[index].getAttribute("data-ohm-tag-color");
          catalog.set(key, { key: key, label: label, search: foldTag(label), color: /^[0-7]$/.test(color) ? color : null });
        }
      });
      return { element: element, tags: tags };
    });
    if (!catalog.size) return;

    var selected = new Set();
    var groups = Array.from(scope.querySelectorAll("[data-ohm-todo-group]"));
    var collected = scope.querySelectorAll("[data-ohm-todo-collected]");
    var collator = new Intl.Collator(undefined, { sensitivity: "base" });
    function alphabetically(a, b) {
      return collator.compare(a.label, b.label) || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
    }
    var tagRecords = Array.from(catalog.values()).sort(alphabetically);
    var listId = "ohm-todo-tags";
    while (document.getElementById(listId)) listId += "-list";
    tagRecords.forEach(function (tag, index) {
      tag.id = listId + "-" + index;
      while (document.getElementById(tag.id)) tag.id += "-tag";
    });
    var open = false;
    var composing = false;
    var activeIndex = -1;
    var visibleTags = [];
    var optionElements = [];

    function appendIcon(parent, name) {
      var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      svg.setAttribute("class", "ohm-icon");
      svg.setAttribute("viewBox", "0 0 24 24");
      svg.setAttribute("aria-hidden", "true");
      svg.setAttribute("focusable", "false");
      var shapes = name === "search"
        ? [["circle", { cx: "10.5", cy: "10.5", r: "6.5" }], ["path", { d: "m16 16 4 4" }]]
        : [["path", { d: name === "check" ? "M20 6 9 17l-5-5" : "m6 6 12 12M18 6 6 18" }]];
      shapes.forEach(function (shape) {
        var element = document.createElementNS("http://www.w3.org/2000/svg", shape[0]);
        Object.keys(shape[1]).forEach(function (key) { element.setAttribute(key, shape[1][key]); });
        svg.appendChild(element);
      });
      parent.appendChild(svg);
    }

    function copyColor(element, tag) {
      if (tag.color !== null) element.setAttribute("data-ohm-tag-color", tag.color);
    }

    var toolbar = document.createElement("div");
    toolbar.className = "ohm-todo-filter";
    toolbar.setAttribute("role", "search");
    toolbar.setAttribute("aria-label", "Filter tasks by tags");
    var picker = document.createElement("div");
    picker.className = "ohm-todo-picker";
    toolbar.appendChild(picker);
    var inputRow = document.createElement("div");
    inputRow.className = "ohm-todo-input-row";
    appendIcon(inputRow, "search");
    picker.appendChild(inputRow);
    var selectedTags = document.createElement("div");
    selectedTags.className = "ohm-todo-selected";
    inputRow.appendChild(selectedTags);
    var search = document.createElement("input");
    search.className = "ohm-todo-search";
    search.type = "text";
    search.setAttribute("role", "combobox");
    search.setAttribute("aria-label", "Filter by tags");
    search.setAttribute("placeholder", "Filter tags…");
    search.setAttribute("autocomplete", "off");
    search.setAttribute("aria-autocomplete", "list");
    search.setAttribute("aria-controls", listId);
    search.setAttribute("aria-expanded", "false");
    inputRow.appendChild(search);
    var clear = document.createElement("button");
    clear.type = "button";
    clear.className = "ohm-todo-clear";
    clear.setAttribute("aria-label", "Clear tag filter");
    clear.setAttribute("title", "Clear");
    appendIcon(clear, "close");
    inputRow.appendChild(clear);
    var count = document.createElement("span");
    count.className = "ohm-todo-count";
    count.setAttribute("role", "status");
    count.setAttribute("aria-live", "polite");
    count.setAttribute("aria-atomic", "true");
    toolbar.appendChild(count);

    var popup = document.createElement("div");
    popup.className = "ohm-todo-popup";
    setHidden(popup, true);
    picker.appendChild(popup);
    var options = document.createElement("ul");
    options.className = "ohm-todo-options";
    options.id = listId;
    options.setAttribute("role", "listbox");
    options.setAttribute("aria-label", "Tags");
    options.setAttribute("aria-multiselectable", "true");
    popup.appendChild(options);
    var empty = document.createElement("div");
    empty.className = "ohm-todo-empty";
    empty.textContent = "No matching tags";
    popup.appendChild(empty);
    var hint = document.createElement("div");
    hint.className = "ohm-todo-hint";
    popup.appendChild(hint);

    function syncActive(scroll) {
      optionElements.forEach(function (element, index) {
        element.classList.toggle("ohm-todo-option--active", open && index === activeIndex);
      });
      var active = open && optionElements[activeIndex];
      if (active) {
        search.setAttribute("aria-activedescendant", active.id);
        if (scroll && typeof active.scrollIntoView === "function") active.scrollIntoView({ block: "nearest" });
      } else search.removeAttribute("aria-activedescendant");
    }

    function searchRank(tag, query) {
      if (tag.search === query) return 0;
      if (tag.search.startsWith(query)) return 1;
      if (tag.search.includes(query)) return 2;
      var next = 0;
      for (var index = 0; index < tag.search.length && next < query.length; index++) {
        if (tag.search[index] === query[next]) next++;
      }
      return next === query.length ? 3 : -1;
    }

    function renderOptions() {
      var query = foldTag(search.value.trim());
      var ranked = tagRecords.map(function (tag) { return { tag: tag, rank: query ? searchRank(tag, query) : 0 }; })
        .filter(function (entry) { return entry.rank >= 0; });
      ranked.sort(function (a, b) {
        var priority = query ? a.rank - b.rank : Number(selected.has(b.tag.key)) - Number(selected.has(a.tag.key));
        return priority || alphabetically(a.tag, b.tag);
      });
      visibleTags = ranked.slice(0, 8).map(function (entry) { return entry.tag; });
      options.textContent = "";
      optionElements = visibleTags.map(function (tag, index) {
        var option = document.createElement("li");
        option.className = "ohm-todo-option";
        option.id = tag.id;
        option.setAttribute("role", "option");
        option.setAttribute("aria-selected", selected.has(tag.key) ? "true" : "false");
        var label = document.createElement("span");
        copyColor(label, tag);
        label.className = "ohm-task-tag";
        label.textContent = tag.label;
        option.appendChild(label);
        if (selected.has(tag.key)) appendIcon(option, "check");
        option.addEventListener("pointerdown", function (event) { event.preventDefault(); });
        option.addEventListener("pointermove", function () { activeIndex = index; syncActive(false); });
        option.addEventListener("click", function () { toggleTag(tag.key); });
        options.appendChild(option);
        return option;
      });
      activeIndex = visibleTags.length ? 0 : -1;
      setHidden(empty, !!visibleTags.length);
      setHidden(hint, !ranked.length);
      hint.textContent = ranked.length > 8 ? "Showing 8 of " + ranked.length + " tags. Type to narrow." :
        ranked.length + (ranked.length === 1 ? " tag" : " tags") + " · Enter to toggle";
      clear.disabled = !selected.size && !search.value;
      syncActive(false);
    }

    function openPopup() {
      open = true;
      setHidden(popup, false);
      search.setAttribute("aria-expanded", "true");
      renderOptions();
    }

    function closePopup() {
      open = false;
      setHidden(popup, true);
      search.setAttribute("aria-expanded", "false");
      activeIndex = -1;
      syncActive(false);
    }

    function renderSelected() {
      selectedTags.textContent = "";
      var tags = Array.from(selected).map(function (key) { return catalog.get(key); });
      tags.slice(0, 2).forEach(function (tag) {
        var button = document.createElement("button");
        copyColor(button, tag);
        button.className = "ohm-todo-selected-tag";
        button.type = "button";
        button.setAttribute("aria-label", "Remove " + tag.label);
        button.setAttribute("title", "Remove " + tag.label);
        var label = document.createElement("span");
        label.textContent = tag.label;
        button.appendChild(label);
        appendIcon(button, "close");
        button.addEventListener("click", function () { toggleTag(tag.key); });
        selectedTags.appendChild(button);
      });
      if (tags.length > 2) {
        var more = document.createElement("button");
        more.type = "button";
        more.className = "ohm-todo-more";
        more.textContent = "+" + (tags.length - 2);
        more.setAttribute("title", tags.slice(2).map(function (tag) { return tag.label; }).join(", "));
        more.setAttribute("aria-label", "Show " + (tags.length - 2) + " more selected tags");
        more.addEventListener("click", function () { search.value = ""; search.focus(); openPopup(); });
        selectedTags.appendChild(more);
      }
    }

    function toggleTag(key) {
      if (selected.has(key)) selected.delete(key);
      else selected.add(key);
      search.value = "";
      renderSelected();
      applyFilter();
      search.focus();
      openPopup();
    }

    search.addEventListener("focus", openPopup);
    search.addEventListener("click", function () { if (!open) openPopup(); });
    search.addEventListener("input", openPopup);
    search.addEventListener("compositionstart", function () { composing = true; });
    search.addEventListener("compositionend", function () { composing = false; openPopup(); });
    search.addEventListener("keydown", function (event) {
      if (composing || event.isComposing || event.keyCode === 229) return;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        var wasOpen = open;
        if (!open) openPopup();
        if (visibleTags.length) {
          var step = event.key === "ArrowDown" ? 1 : -1;
          activeIndex = wasOpen ? (activeIndex + step + visibleTags.length) % visibleTags.length :
            (step > 0 ? 0 : visibleTags.length - 1);
          syncActive(true);
        }
      } else if (event.key === "Enter" && open && activeIndex >= 0) {
        event.preventDefault();
        toggleTag(visibleTags[activeIndex].key);
      } else if (event.key === "Escape" && open) {
        event.preventDefault();
        event.stopPropagation();
        closePopup();
      } else if (event.key === "Tab") closePopup();
    });
    picker.addEventListener("focusout", function (event) {
      if (!picker.contains(event.relatedTarget)) closePopup();
    });
    document.addEventListener("pointerdown", function (event) {
      if (!picker.contains(event.target)) closePopup();
    });

    // Only direct children can be the page title/subtitle; collected headings
    // and headings inside tasks must not pull the controls into their content.
    var children = Array.from(scope.children);
    var title = children.find(function (child) { return child.classList.contains("ohm-page-filename"); }) ||
      children.find(function (child) { return child.tagName === "H1"; });
    scope.insertBefore(toolbar, title ? title.nextSibling : scope.firstChild);

    function applyFilter() {
      var matches = new Set();
      var visible = new Set();
      tasks.forEach(function (task) {
        var matchesTag = !selected.size || Array.from(task.tags).some(function (key) { return selected.has(key); });
        if (!matchesTag) return;
        matches.add(task.element);
        // Keep every task ancestor for context, without making its unrelated
        // descendants match. The count includes direct matches only.
        var ancestor = task.element;
        while (ancestor && scope.contains(ancestor)) {
          visible.add(ancestor);
          ancestor = ancestor.parentElement && ancestor.parentElement.closest("li.ohm-task");
        }
      });
      tasks.forEach(function (task) { setHidden(task.element, !visible.has(task.element)); });
      groups.forEach(function (group) {
        var hasMatch = tasks.some(function (task) { return matches.has(task.element) && group.contains(task.element); });
        setHidden(group, !!selected.size && !hasMatch);
      });
      collected.forEach(function (section) {
        var hasGroup = groups.some(function (group) { return section.contains(group) && !group.hasAttribute("hidden"); });
        setHidden(section, !!selected.size && !hasGroup);
      });
      clear.disabled = !selected.size && !search.value;
      count.textContent = matches.size + "/" + tasks.length;
      var description = matches.size + " of " + tasks.length + (tasks.length === 1 ? " task matches" : " tasks match");
      count.setAttribute("aria-label", description);
      count.setAttribute("title", description);
      refreshOutline();
    }

    clear.addEventListener("click", function () {
      selected.clear();
      search.value = "";
      renderSelected();
      applyFilter();
      search.focus();
      openPopup();
    });
    applyFilter();
    renderOptions();
  }

  enhanceTodoFiltering();

  function enhanceTodoSorting() {
    var scope = document.querySelector("article[data-ohm-todo]");
    if (!scope) return;
    var parent = scope.querySelector("[data-ohm-todo-collected]") || scope;
    var sections = Array.from(parent.children).filter(function (child) {
      return child.tagName === "SECTION" && child.hasAttribute("data-ohm-todo-group");
    });
    if (sections.length < 2) return;

    var modes = new Map([
      ["default", "Default (filename order)"],
      ["name-asc", "Name A–Z"],
      ["name-desc", "Name Z–A"],
      ["created-desc", "Created newest"],
      ["created-asc", "Created oldest"],
      ["updated-desc", "Updated newest"],
      ["updated-asc", "Updated oldest"],
    ]);
    var collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
    function timestamp(section, attribute) {
      var value = section.getAttribute(attribute);
      if (value === null || !/^-?\d+$/.test(value)) return null;
      var number = Number(value);
      return Number.isSafeInteger(number) ? number : null;
    }
    var items = sections.map(function (section, index) {
      var heading = section.querySelector("h2");
      return {
        element: section,
        index: index,
        title: heading ? heading.textContent.trim() : "",
        created: timestamp(section, "data-ohm-created"),
        updated: timestamp(section, "data-ohm-updated"),
      };
    });

    // Outline rows are flat, but each file owns every heading inside its
    // section, including headings nested in tasks. Move that complete block.
    var outlines = new Map();
    items.forEach(function (item) {
      item.element.querySelectorAll("h1, h2, h3, h4, h5, h6").forEach(function (heading) {
        var row = itemByHeadingId.get(heading.id);
        if (!row || !row.parentElement) return;
        var list = row.parentElement;
        if (!outlines.has(list)) outlines.set(list, { children: Array.from(list.children), owners: new Map() });
        outlines.get(list).owners.set(row, item);
      });
    });

    var toolbar = scope.querySelector(".ohm-todo-filter");
    if (!toolbar) {
      toolbar = document.createElement("div");
      toolbar.className = "ohm-todo-controls";
      var direct = Array.from(scope.children);
      var title = direct.find(function (child) { return child.classList.contains("ohm-page-filename"); }) ||
        direct.find(function (child) { return child.tagName === "H1"; });
      scope.insertBefore(toolbar, title ? title.nextSibling : scope.firstChild);
    }
    var control = document.createElement("label");
    control.className = "ohm-icon-button ohm-nav-sort ohm-todo-sort";
    control.innerHTML = ICONS.sort;
    var select = document.createElement("select");
    select.className = "ohm-nav-sort-select ohm-todo-sort-select";
    select.setAttribute("aria-label", "Sort TODO files");
    modes.forEach(function (label, value) {
      var option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    });
    control.appendChild(select);
    toolbar.appendChild(control);
    var children = Array.from(parent.children);
    var sortableElements = new Set(sections);

    function moveChildren(list, ordered) {
      ordered.forEach(function (element, index) {
        var reference = list.children[index] || null;
        if (element === reference) return;
        if (typeof list.moveBefore === "function") list.moveBefore(element, reference);
        else list.insertBefore(element, reference);
      });
    }

    function sort(mode) {
      var ordered = items.slice();
      if (mode !== "default") {
        var field = mode.split("-")[0];
        var direction = mode.endsWith("-desc") ? -1 : 1;
        ordered.sort(function (a, b) {
          if (field !== "name") {
            if (a[field] === null && b[field] !== null) return 1;
            if (a[field] !== null && b[field] === null) return -1;
            if (a[field] !== null && a[field] !== b[field]) {
              return (a[field] < b[field] ? -1 : 1) * direction;
            }
          }
          return collator.compare(a.title, b.title) * (field === "name" ? direction : 1) || a.index - b.index;
        });
      }
      var next = 0;
      moveChildren(parent, children.map(function (child) {
        return sortableElements.has(child) ? ordered[next++].element : child;
      }));
      outlines.forEach(function (outline, list) {
        var blocks = new Map();
        outline.children.forEach(function (row) {
          var owner = outline.owners.get(row);
          if (!owner) return;
          if (!blocks.has(owner)) blocks.set(owner, []);
          blocks.get(owner).push(row);
        });
        var sortedBlocks = ordered.filter(function (item) { return blocks.has(item); });
        var seen = new Set();
        var rows = [];
        var nextBlock = 0;
        outline.children.forEach(function (row) {
          var owner = outline.owners.get(row);
          if (!owner) rows.push(row);
          else if (!seen.has(owner)) {
            seen.add(owner);
            rows.push.apply(rows, blocks.get(sortedBlocks[nextBlock++]));
          }
        });
        moveChildren(list, rows);
      });
      select.value = mode;
      control.setAttribute("data-ohm-sort-mode", mode);
      control.classList.toggle("ohm-nav-sort--active", mode !== "default");
      var title = "Sort TODO files: " + modes.get(mode);
      control.setAttribute("title", title);
      select.setAttribute("title", title);
      outlineHeadings = Array.from(document.querySelectorAll('.ohm-outline-item > a[href^="#"]')).map(function (link) {
        return document.getElementById((link.getAttribute("href") || "").slice(1));
      }).filter(function (heading) { return !!heading; });
      refreshOutline();
    }

    var brand = document.querySelector(".ohm-brand[href]");
    var storageKey = brand ? "ohimark-todo-sort:" + brand.href : null;
    var storedValue = null;
    if (storageKey) {
      try { storedValue = window.sessionStorage.getItem(storageKey); }
      catch (err) { /* Sorting remains available with blocked storage. */ }
    }
    function writePreference(mode) {
      var value = mode === "default" ? null : mode;
      if (!storageKey || value === storedValue) return;
      try {
        if (value === null) window.sessionStorage.removeItem(storageKey);
        else window.sessionStorage.setItem(storageKey, value);
        storedValue = value;
      } catch (err) { /* Persistence is best-effort only. */ }
    }
    var initialMode = modes.has(storedValue) ? storedValue : "default";
    sort(initialMode);
    writePreference(initialMode);
    select.addEventListener("change", function () {
      var mode = modes.has(select.value) ? select.value : "default";
      sort(mode);
      writePreference(mode);
    });
  }

  enhanceTodoSorting();

  /* =======================================================================
     6. Sidebar controls: preserve independent preferences while keeping
     the mobile navigation drawer closed when entering the mobile layout.
     ======================================================================= */

  var navState = document.getElementById("ohm-nav-state");
  var outlineState = document.getElementById("ohm-outline-state");
  var navButton = null;
  var outlineButton = null;
  var navMedia = null;
  var desktopSidebarCollapsed = false;
  var navigationWasMobile = false;
  var refreshSidebarGroupSizes = function () {};
  var refreshTableSizes = function () {};

  function readPanelCollapsed(key) {
    try {
      return window.sessionStorage.getItem(key) === "true";
    } catch (err) {
      /* With no stored preference, panels start expanded on desktop. */
      return false;
    }
  }

  function writePanelCollapsed(key, collapsed) {
    try {
      window.sessionStorage.setItem(key, collapsed ? "true" : "false");
    } catch (err) {
      /* Ignore: persistence is best-effort, including on file:// pages. */
    }
  }

  function enhancePanelButton(state, selector, panelId) {
    if (!state) return null;
    var label = document.querySelector(selector);
    if (!label) return null;
    var button = document.createElement("button");
    button.type = "button";
    button.className = label.className;
    while (label.firstChild) {
      button.appendChild(label.firstChild);
    }
    button.setAttribute("aria-controls", panelId);
    label.parentNode.replaceChild(button, label);
    state.setAttribute("tabindex", "-1");
    state.setAttribute("aria-hidden", "true");
    return button;
  }

  function syncPanelButton(button, label, expanded) {
    button.setAttribute("aria-expanded", expanded ? "true" : "false");
    button.setAttribute("aria-label", label);
    button.setAttribute("title", label);
    if (!expanded) {
      var panel = document.getElementById(button.getAttribute("aria-controls"));
      if (panel && panel.contains(document.activeElement) && document.activeElement !== button) {
        button.focus();
      }
    }
  }

  try {
    if (typeof window.matchMedia === "function") {
      navMedia = window.matchMedia("(max-width: 960px)");
    }
  } catch (err) {
    /* Use the viewport width if media queries are unavailable. */
  }

  function navigationIsMobile() {
    return navMedia
      ? navMedia.matches
      : (window.innerWidth || document.documentElement.clientWidth) <= 960;
  }

  function syncNavigationButton() {
    if (!navState || !navButton) return;
    var mobile = navigationIsMobile();
    var expanded = mobile ? navState.checked : !navState.checked;
    var label = mobile
      ? (expanded ? "Close navigation" : "Open navigation")
      : (expanded ? "Collapse navigation" : "Expand navigation");
    syncPanelButton(navButton, label, expanded);
    refreshSidebarGroupSizes();
    refreshTableSizes();
  }

  function toggleNavigation() {
    if (!navState || !navButton) return;
    navState.checked = !navState.checked;
    if (!navigationIsMobile()) {
      desktopSidebarCollapsed = navState.checked;
      writePanelCollapsed("ohimark-sidebar-collapsed", desktopSidebarCollapsed);
    }
    syncNavigationButton();
  }

  function syncNavigationViewport() {
    if (!navState || !navButton) return;
    var mobile = navigationIsMobile();
    if (mobile !== navigationWasMobile) {
      navState.checked = mobile ? false : desktopSidebarCollapsed;
      navigationWasMobile = mobile;
    }
    syncNavigationButton();
  }

  navButton = enhancePanelButton(navState, 'label.ohm-nav-button[for="ohm-nav-state"]', "ohm-sidebar");
  if (navButton) {
    desktopSidebarCollapsed = readPanelCollapsed("ohimark-sidebar-collapsed");
    navigationWasMobile = navigationIsMobile();
    navState.checked = navigationWasMobile ? false : desktopSidebarCollapsed;
    syncNavigationButton();

    if (navMedia && typeof navMedia.addEventListener === "function") {
      navMedia.addEventListener("change", syncNavigationViewport);
    } else if (navMedia && typeof navMedia.addListener === "function") {
      navMedia.addListener(syncNavigationViewport);
    } else {
      window.addEventListener("resize", syncNavigationViewport);
    }

    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && navigationIsMobile() && navState.checked) {
        navState.checked = false;
        syncNavigationButton();
        navButton.focus();
      }
    });
  }

  function syncOutlineButton() {
    if (!outlineState || !outlineButton) return;
    var expanded = !outlineState.checked;
    syncPanelButton(outlineButton, expanded ? "Collapse page outline" : "Expand page outline", expanded);
    refreshTableSizes();
  }

  outlineButton = enhancePanelButton(outlineState, 'label.ohm-outline-button[for="ohm-outline-state"]', "ohm-outline");
  if (outlineButton) {
    outlineState.checked = readPanelCollapsed("ohimark-outline-collapsed");
    syncOutlineButton();
  }

  function preserveSidebarGroups() {
    var groups = document.querySelectorAll(".ohm-sidebar details.ohm-nav-details[data-ohm-nav-group]");
    var brand = document.querySelector(".ohm-brand[href]");
    if (!groups.length || !brand) return;

    // The resolved brand URL identifies the site even from nested pages.
    var storageKey = "ohimark-sidebar-groups:" + brand.href;
    var groupKeys = new Set();
    var collapsedKeys = new Set();
    var storedValue = null;
    groups.forEach(function (group) {
      groupKeys.add(group.getAttribute("data-ohm-nav-group"));
    });

    try {
      storedValue = window.sessionStorage.getItem(storageKey);
      var storedKeys = JSON.parse(storedValue);
      if (Array.isArray(storedKeys)) {
        storedKeys.forEach(function (key) {
          if (typeof key === "string" && groupKeys.has(key)) {
            collapsedKeys.add(key);
          }
        });
      }
    } catch (err) {
      /* Unavailable storage or invalid JSON must not disable native groups. */
    }

    function writeCollapsedGroups() {
      var value = collapsedKeys.size ? JSON.stringify(Array.from(collapsedKeys)) : null;
      if (value === storedValue) return;
      try {
        if (value === null) {
          window.sessionStorage.removeItem(storageKey);
        } else {
          window.sessionStorage.setItem(storageKey, value);
        }
        storedValue = value;
      } catch (err) {
        /* Persistence is best-effort, including blocked storage and quotas. */
      }
    }

    groups.forEach(function (group) {
      var key = group.getAttribute("data-ohm-nav-group");
      if (collapsedKeys.has(key)) group.open = false;
      var wasOpen = group.open;
      group.addEventListener("toggle", function () {
        // Initial and restoration toggles arrive asynchronously. Only an
        // actual change from the restored state represents a new choice.
        if (group.open === wasOpen) return;
        wasOpen = group.open;
        if (wasOpen) {
          collapsedKeys.delete(key);
        } else {
          collapsedKeys.add(key);
        }
        writeCollapsedGroups();
      });
    });

    // Drop obsolete keys and empty entries without writing on a first visit.
    writeCollapsedGroups();
  }

  preserveSidebarGroups();

  function enhanceSidebarSorting() {
    var brand = document.querySelector(".ohm-brand[href]");
    if (!brand) return;

    var modes = new Map([
      ["default", "Default (filename order)"],
      ["name-asc", "Name A–Z"],
      ["name-desc", "Name Z–A"],
      ["created-desc", "Created newest"],
      ["created-asc", "Created oldest"],
      ["updated-desc", "Updated newest"],
      ["updated-asc", "Updated oldest"],
    ]);
    var groups = [];
    var preferences = Object.create(null);
    var storageKey = "ohimark-sidebar-sort:" + brand.href;
    var storedValue = null;
    var collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

    function directChild(parent, className) {
      return Array.from(parent.children).find(function (child) {
        return child.classList.contains(className);
      });
    }

    var topList = document.querySelector(".ohm-nav > .ohm-nav-list");
    var header = document.querySelector(".ohm-sidebar-header");
    if (topList && header) {
      groups.push({ key: "", list: topList, heading: header, label: "Sort top-level items" });
    }
    document.querySelectorAll(".ohm-sidebar details.ohm-nav-details[data-ohm-nav-group]").forEach(function (group) {
      var list = directChild(group, "ohm-nav-list");
      var summary = directChild(group, "ohm-nav-summary");
      if (!list || !summary) return;
      var label = directChild(summary, "ohm-nav-label");
      var key = group.getAttribute("data-ohm-nav-group");
      groups.push({
        key: key,
        list: list,
        heading: summary,
        container: group.parentElement,
        label: "Sort items in " + (label ? label.textContent.trim() : key),
      });
    });
    if (!groups.length) return;

    var groupKeys = new Set(groups.map(function (group) { return group.key; }));
    try {
      storedValue = window.sessionStorage.getItem(storageKey);
      var saved = JSON.parse(storedValue);
      if (saved && typeof saved === "object" && !Array.isArray(saved)) {
        Object.keys(saved).forEach(function (key) {
          if (groupKeys.has(key) && saved[key] !== "default" && modes.has(saved[key])) {
            preferences[key] = saved[key];
          }
        });
      }
    } catch (err) {
      /* Sorting remains available with blocked storage or malformed JSON. */
    }

    function writePreferences() {
      var value = Object.keys(preferences).length ? JSON.stringify(preferences) : null;
      if (value === storedValue) return;
      try {
        if (value === null) {
          window.sessionStorage.removeItem(storageKey);
        } else {
          window.sessionStorage.setItem(storageKey, value);
        }
        storedValue = value;
      } catch (err) {
        /* Persistence is best-effort; the current sort still takes effect. */
      }
    }

    function timestamp(item, attribute) {
      var value = item.getAttribute(attribute);
      if (value === null || !/^-?\d+$/.test(value)) return null;
      var number = Number(value);
      return Number.isSafeInteger(number) ? number : null;
    }

    groups.forEach(function (group) {
      var children = Array.from(group.list.children);
      var items = [];
      children.forEach(function (child, index) {
        if (child.tagName !== "LI") return;
        var label;
        if (child.classList.contains("ohm-nav-group")) {
          var details = directChild(child, "ohm-nav-details");
          var summary = details && directChild(details, "ohm-nav-summary");
          label = summary && directChild(summary, "ohm-nav-label");
        } else if (child.classList.contains("ohm-nav-item")) {
          label = directChild(child, "ohm-nav-link");
        } else {
          return;
        }
        items.push({
          element: child,
          index: index,
          title: label ? label.textContent.trim() : "",
          created: timestamp(child, "data-ohm-created"),
          updated: timestamp(child, "data-ohm-updated"),
        });
      });
      if (items.length < 2) return;
      var sortableElements = new Set(items.map(function (item) { return item.element; }));

      var control = document.createElement("label");
      control.className = "ohm-icon-button ohm-nav-sort";
      control.innerHTML = ICONS.sort;
      var select = document.createElement("select");
      select.className = "ohm-nav-sort-select";
      select.setAttribute("aria-label", group.label);
      modes.forEach(function (label, value) {
        var option = document.createElement("option");
        option.value = value;
        option.textContent = label;
        select.appendChild(option);
      });
      control.appendChild(select);
      if (group.container) {
        // A select inside summary can toggle details on native keyboard input.
        // Keep it beside details so the chooser also works for closed groups.
        group.container.classList.add("ohm-nav-group--sortable");
        group.container.insertBefore(control, group.container.firstChild);
      } else {
        group.heading.insertBefore(control, directChild(group.heading, "ohm-nav-button") || null);
      }
      ["click", "keydown", "keyup"].forEach(function (type) {
        control.addEventListener(type, function (event) { event.stopPropagation(); });
      });

      function sort(mode) {
        var ordered = items.slice();
        if (mode !== "default") {
          var field = mode.split("-")[0];
          var direction = mode.endsWith("-desc") ? -1 : 1;
          ordered.sort(function (a, b) {
            if (field !== "name") {
              if (a[field] === null && b[field] !== null) return 1;
              if (a[field] !== null && b[field] === null) return -1;
              if (a[field] !== null && a[field] !== b[field]) {
                return (a[field] < b[field] ? -1 : 1) * direction;
              }
            }
            return collator.compare(a.title, b.title) * (field === "name" ? direction : 1) || a.index - b.index;
          });
        }
        // Keep unrecognized children in their original slots. Move existing
        // rows only when needed; moveBefore also preserves native DOM state.
        var next = 0;
        children.forEach(function (original, index) {
          var element = sortableElements.has(original) ? ordered[next++].element : original;
          var reference = group.list.children[index] || null;
          if (element === reference) return;
          if (typeof group.list.moveBefore === "function") {
            group.list.moveBefore(element, reference);
          } else {
            group.list.insertBefore(element, reference);
          }
        });
        select.value = mode;
        control.setAttribute("data-ohm-sort-mode", mode);
        control.classList.toggle("ohm-nav-sort--active", mode !== "default");
        var title = group.label + ": " + modes.get(mode);
        control.setAttribute("title", title);
        select.setAttribute("title", title);
        refreshSidebarGroupSizes();
      }

      sort(preferences[group.key] || "default");
      select.addEventListener("change", function () {
        var mode = modes.has(select.value) ? select.value : "default";
        if (mode === "default") {
          delete preferences[group.key];
        } else {
          preferences[group.key] = mode;
        }
        sort(mode);
        writePreferences();
      });
    });

    // Clean stale/default/invalid entries while leaving first visits unwritten.
    writePreferences();
  }

  enhanceSidebarSorting();

  function enhanceSidebarGroupScrolling() {
    var nav = document.querySelector(".ohm-sidebar .ohm-nav");
    if (!nav || typeof nav.getBoundingClientRect !== "function" ||
        typeof window.getComputedStyle !== "function") return;
    var configured = nav.getAttribute("data-ohm-max-group-size");
    var limit = configured && /^\d+$/.test(configured) ? Number(configured) : 10;
    if (!Number.isSafeInteger(limit) || limit < 1) limit = 10;
    var entries = [];
    nav.querySelectorAll("details.ohm-nav-details[data-ohm-nav-group]").forEach(function (group) {
      var list = Array.from(group.children).find(function (child) {
        return child.tagName === "UL" && child.classList.contains("ohm-nav-list");
      });
      if (!list) return;
      var summary = Array.from(group.children).find(function (child) { return child.tagName === "SUMMARY"; });
      var label = summary && summary.querySelector(".ohm-nav-label");
      var entry = {
        group: group,
        list: list,
        label: label ? label.textContent.trim() : group.getAttribute("data-ohm-nav-group"),
        maxHeight: list.style.maxHeight || "",
        tabindex: list.getAttribute("tabindex"),
        ariaLabel: list.getAttribute("aria-label"),
        topCue: null,
        bottomCue: null,
        revealed: false,
      };
      entries.push(entry);
      group.addEventListener("toggle", function () { refreshSidebarGroupSizes(); });
      list.addEventListener("scroll", function () { updateCue(entry); });
    });
    if (!entries.length) return;
    // Document order visits parents first. Descendants must settle before an
    // ancestor measures a row containing an expanded subgroup.
    entries.reverse();

    function pixels(style, name) { return parseFloat(style[name]) || 0; }

    function setCueVisibility(cue, visible) {
      cue.style.visibility = visible ? "visible" : "hidden";
      cue.style.pointerEvents = visible ? "auto" : "none";
      cue.setAttribute("aria-hidden", visible ? "false" : "true");
    }

    function createCue(className, description) {
      var cue = document.createElement("span");
      cue.className = "ohm-nav-overflow " + className;
      cue.textContent = "...";
      cue.setAttribute("role", "img");
      cue.setAttribute("title", description);
      cue.setAttribute("aria-label", description);
      setCueVisibility(cue, false);
      return cue;
    }

    function updateCue(entry) {
      if (!entry.topCue) return;
      var list = entry.list;
      var bounds = list.getBoundingClientRect();
      var range = Math.max(0, list.scrollHeight - list.clientHeight);
      // Clamp elastic overscroll and tolerate fractional rounding at either
      // edge. Visibility keeps both slots' geometry stable while scrolling.
      var top = Math.max(0, Math.min(list.scrollTop, range));
      var overflow = list.classList.contains("ohm-nav-list--scroll") &&
        bounds.width > 0 && bounds.height > 0 && range > 1;
      setCueVisibility(entry.topCue, overflow && top > 1);
      setCueVisibility(entry.bottomCue, overflow && range - top > 1);
    }

    function restoreAttribute(element, name, value) {
      if (value === null) element.removeAttribute(name);
      else element.setAttribute(name, value);
    }

    function uncap(entry) {
      entry.list.style.maxHeight = entry.maxHeight;
      entry.list.classList.remove("ohm-nav-list--scroll");
      restoreAttribute(entry.list, "tabindex", entry.tabindex);
      restoreAttribute(entry.list, "aria-label", entry.ariaLabel);
      if (entry.topCue) {
        [entry.topCue, entry.bottomCue].forEach(function (cue) {
          cue.setAttribute("hidden", "");
          setCueVisibility(cue, false);
        });
      }
    }

    function measure(entry) {
      var list = entry.list;
      var rows = Array.from(list.children).filter(function (child) {
        return child.tagName === "LI" &&
          (child.classList.contains("ohm-nav-item") || child.classList.contains("ohm-nav-group"));
      });
      if (rows.length <= limit) {
        uncap(entry);
        return;
      }
      list.classList.add("ohm-nav-list--scroll");
      // Force the eventual scrollbar width during measurement, including in
      // browsers without scrollbar-gutter. These synchronous changes do not
      // paint an intermediate viewport or depend on a fixed row height.
      list.style.maxHeight = "1px";
      var bounds = list.getBoundingClientRect();
      var last = rows[limit - 1];
      var lastBounds = last.getBoundingClientRect();
      var style = window.getComputedStyle(list);
      var height = lastBounds.bottom - bounds.top + list.scrollTop +
        pixels(style, "borderBottomWidth");
      if (!bounds.width || !bounds.height || !lastBounds.height || !Number.isFinite(height) || height <= 0) {
        // Closed disclosures, a hidden sidebar, or missing layout must leave
        // the original navigation usable until a later visible measurement.
        uncap(entry);
        return;
      }
      // Bottom padding belongs after the complete list, not after the Nth
      // row: including it here would expose part of the next row. Invisible
      // trailing rows also must not create a viewport for padding alone.
      if (!rows.slice(limit).some(function (row) { return row.getBoundingClientRect().height > 0; })) {
        uncap(entry);
        return;
      }
      list.style.maxHeight = Math.ceil(height) + "px";
      if (list.scrollHeight - list.clientHeight <= 1) {
        uncap(entry);
        return;
      }
      if (entry.tabindex === null) list.setAttribute("tabindex", "0");
      if (entry.ariaLabel === null) list.setAttribute("aria-label", "Items in " + entry.label + ". Scroll for more.");
      if (!entry.topCue) {
        entry.topCue = createCue("ohm-nav-overflow--top", "More items above. Scroll within this group.");
        entry.bottomCue = createCue("ohm-nav-overflow--bottom", "More items below. Scroll within this group.");
        entry.group.insertBefore(entry.topCue, list);
        entry.group.appendChild(entry.bottomCue);
      }
      // Both slots must be present before an ancestor measures this row;
      // switching direction later changes visibility without moving content.
      entry.topCue.removeAttribute("hidden");
      entry.bottomCue.removeAttribute("hidden");
    }

    function revealCurrent(entry) {
      if (entry.revealed) return;
      var list = entry.list;
      var current = list.querySelector('[aria-current="page"]');
      if (!current) { entry.revealed = true; return; }
      var bounds = list.getBoundingClientRect();
      var target = current.getBoundingClientRect();
      if (!bounds.width || !bounds.height || !target.height) return;
      if (list.classList.contains("ohm-nav-list--scroll")) {
        var top = bounds.top + (list.clientTop || 0);
        var bottom = top + list.clientHeight;
        // Assign only this list's scroll position: scrollIntoView would also
        // move the article/window and can expose restored-closed ancestors.
        if (target.bottom > bottom) list.scrollTop += target.bottom - bottom;
        if (target.top < top) list.scrollTop += target.top - top;
        updateCue(entry);
      }
      entry.revealed = true;
    }

    function refresh() {
      // A temporary cap on a descendant can shrink an ancestor's scroll
      // range and clamp its scrollTop before that ancestor is measured.
      // Snapshot every list, including the root viewport, before any writes.
      var positions = Array.from(nav.querySelectorAll("ul.ohm-nav-list")).map(function (list) {
        return { list: list, top: list.scrollTop };
      });
      entries.forEach(measure);
      positions.forEach(function (position) { position.list.scrollTop = position.top; });
      entries.forEach(revealCurrent);
      entries.forEach(updateCue);
    }

    var scheduled = false;
    refreshSidebarGroupSizes = function () {
      if (scheduled) return;
      scheduled = true;
      function run() { scheduled = false; refresh(); }
      if (typeof window.requestAnimationFrame === "function") window.requestAnimationFrame(run);
      else run();
    };
    window.addEventListener("resize", refreshSidebarGroupSizes);
    if (navState) navState.addEventListener("change", refreshSidebarGroupSizes);
    if (typeof window.ResizeObserver === "function") {
      try {
        var observer = new window.ResizeObserver(refreshSidebarGroupSizes);
        observer.observe(nav);
        entries.forEach(function (entry) { observer.observe(entry.list); });
      } catch (err) { /* Resize and disclosure events remain available. */ }
    }
    if (document.fonts) {
      if (document.fonts.ready && typeof document.fonts.ready.then === "function") {
        document.fonts.ready.then(refreshSidebarGroupSizes, function () {});
      }
      if (typeof document.fonts.addEventListener === "function") {
        document.fonts.addEventListener("loadingdone", refreshSidebarGroupSizes);
      }
    }
    refreshSidebarGroupSizes();
  }

  enhanceSidebarGroupScrolling();

  /* =======================================================================
     7. Table columns: independent sizing, collapse and best-effort per-page
     preferences for the current browsing session.
     ======================================================================= */

  function enhanceTables() {
    var tables = document.querySelectorAll(".ohm-content table");
    if (!tables.length) return;

    var minimum = 48;
    var collapsedWidth = 32;
    var entries = [];
    var occurrences = new Map();
    var preferences = Object.create(null);
    var storageKey = null;
    var storedValue = null;
    var active = null;

    function sum(widths) {
      return widths.reduce(function (total, width) { return total + width; }, 0);
    }

    function computed(element) {
      try { return window.getComputedStyle(element); }
      catch (err) { return {}; }
    }

    function number(style, property) {
      return parseFloat(style[property]) || 0;
    }

    function setStyle(element, property, value) {
      if (element.style[property] !== value) element.style[property] = value;
    }

    function measureChrome(entry) {
      var style = computed(entry.table);
      entry.borderLeft = number(style, "borderLeftWidth");
      entry.chrome = entry.borderLeft + number(style, "borderRightWidth");
    }

    function readWidths(entry) {
      if (entry.widths) return entry.widths.slice();
      var widths = entry.headers.map(function (header) {
        return header.getBoundingClientRect().width;
      });
      return widths.every(function (width) { return Number.isFinite(width) && width > 0; })
        ? widths.map(function (width) { return Math.max(minimum, width); }) : null;
    }

    function validWidths(widths, count) {
      return Array.isArray(widths) && widths.length === count && widths.every(function (width) {
        return typeof width === "number" && Number.isFinite(width) && width >= minimum;
      }) && Number.isFinite(sum(widths));
    }

    // Derive a layout from remembered widths and the mask, rather than from
    // the previous toggle's redistribution. Toggle order cannot accumulate
    // rounding or change the widths recovered when columns are reopened.
    function project(expanded, collapsed, total) {
      var hidden = collapsed.filter(function (value) { return value; }).length;
      total = Math.max(total, hidden * collapsedWidth + (expanded.length - hidden) * minimum);
      var remaining = total - hidden * collapsedWidth;
      var widths = collapsed.map(function () { return collapsedWidth; });
      var pending = expanded.map(function (_, index) { return index; }).filter(function (index) { return !collapsed[index]; });
      while (pending.length) {
        var weight = sum(pending.map(function (index) { return expanded[index]; }));
        var scale = remaining / weight;
        var narrow = pending.filter(function (index) { return expanded[index] * scale < minimum; });
        if (!narrow.length) {
          pending.forEach(function (index) { widths[index] = expanded[index] * scale; });
          break;
        }
        narrow.forEach(function (index) { widths[index] = minimum; remaining -= minimum; });
        pending = pending.filter(function (index) { return narrow.indexOf(index) === -1; });
      }
      return widths;
    }

    function validLayout(layout, count) {
      if (Array.isArray(layout)) return validWidths(layout, count);
      if (!layout || typeof layout !== "object" || !Array.isArray(layout.collapsed)
        || layout.collapsed.length !== count || !layout.collapsed.every(function (value) { return typeof value === "boolean"; })
        || !layout.collapsed.some(function (value) { return value; }) || !layout.collapsed.some(function (value) { return !value; })
        || !validWidths(layout.expanded, count) || !Array.isArray(layout.widths) || layout.widths.length !== count) return false;
      if (!layout.widths.every(function (width, index) {
        return typeof width === "number" && Number.isFinite(width)
          && (layout.collapsed[index] ? width === collapsedWidth : width >= minimum);
      }) || !Number.isFinite(sum(layout.widths))) return false;
      var expected = project(layout.expanded, layout.collapsed, sum(layout.widths));
      return layout.widths.every(function (width, index) { return Math.abs(width - expected[index]) <= 0.01; });
    }

    function copyLayout(layout) {
      return Array.isArray(layout) ? layout.slice() : {
        widths: layout.widths.slice(), collapsed: layout.collapsed.slice(), expanded: layout.expanded.slice(),
      };
    }

    // A table viewport can break out of the prose measure, but never out of
    // the main pane. Nested tables retain the containing block's indentation.
    function viewport(entry, outerWidth) {
      var frame = entry.frame.getBoundingClientRect();
      if (!entry.breakout || !entry.main) {
        return { left: frame.left, width: frame.width };
      }
      var article = entry.article.getBoundingClientRect();
      var main = entry.main.getBoundingClientRect();
      var style = computed(entry.main);
      var left = main.left + number(style, "borderLeftWidth") + number(style, "paddingLeft");
      var right = main.right - number(style, "borderRightWidth") - number(style, "paddingRight");
      var available = Math.max(0, right - left);
      if (!(article.width > 0) || !(available > 0)) return { left: frame.left, width: frame.width };
      var width = Math.min(Math.max(article.width, outerWidth || article.width), available);
      return {
        left: Math.max(left, Math.min(article.left + (article.width - width) / 2, right - width)),
        width: width,
        articleLeft: article.left,
      };
    }

    function syncHandles(entry) {
      var widths = readWidths(entry);
      if (!widths) return;
      entry.handles.forEach(function (handle, index) {
        handle.setAttribute("aria-valuenow", String(widths[index]));
        handle.setAttribute("aria-valuetext", Math.round(widths[index]) + " pixels");
      });
      entry.reset.disabled = !entry.widths;
    }

    function syncColumns(entry) {
      var open = entry.collapsed.filter(function (value) { return !value; }).length;
      entry.cells.forEach(function (cells, index) {
        var collapsed = entry.collapsed[index];
        cells.forEach(function (record) {
          if (collapsed && record.content.contains(document.activeElement) && entry.toggles[index]) entry.toggles[index].focus();
          record.cell.classList.toggle("ohm-table-column-collapsed", collapsed);
          record.content.hidden = collapsed;
        });
        var handle = entry.handles[index];
        handle.hidden = collapsed;
        handle.setAttribute("tabindex", collapsed ? "-1" : "0");
        if (collapsed) handle.setAttribute("aria-hidden", "true");
        else handle.removeAttribute("aria-hidden");
        var toggle = entry.toggles[index];
        if (toggle) {
          var label = (collapsed ? "Expand column " : "Collapse column ") + (entry.labels[index] || (index + 1));
          toggle.disabled = !collapsed && open === 1;
          toggle.setAttribute("aria-expanded", collapsed ? "false" : "true");
          toggle.setAttribute("aria-label", label);
          toggle.setAttribute("title", toggle.disabled ? "At least one column must remain expanded" : label);
        }
      });
    }

    function refreshEntry(entry) {
      measureChrome(entry);
      var outer = entry.widths ? sum(entry.widths) + entry.chrome : null;
      if (outer !== null) {
        if (active && active.entry === entry) {
          active.extent = Math.max(active.extent, outer);
          setStyle(active.spacer, "width", active.extent + "px");
        }
        setStyle(entry.table, "width", outer + "px");
        setStyle(entry.canvas, "width", outer + "px");
      }
      var bounds = viewport(entry, outer);
      if (entry.breakout && bounds.articleLeft !== undefined) {
        setStyle(entry.frame, "width", bounds.width + "px");
        setStyle(entry.frame, "marginInlineStart", (bounds.left - bounds.articleLeft) + "px");
        setStyle(entry.frame, "marginInlineEnd", "0px");
      } else {
        setStyle(entry.frame, "width", "100%");
        setStyle(entry.frame, "marginInlineStart", "0px");
        setStyle(entry.frame, "marginInlineEnd", "0px");
      }
      if (outer === null) {
        // Measure automatic layout against the viewport, not the previous
        // canvas width. A table's column minima can make it wider than that
        // viewport; the control footer must follow its actual width.
        setStyle(entry.canvas, "width", "");
        var measured = entry.table.getBoundingClientRect().width;
        if (Number.isFinite(measured) && measured > 0) setStyle(entry.canvas, "width", measured + "px");
      }
      syncHandles(entry);
    }

    function applyWidths(entry, widths) {
      entry.widths = widths ? widths.slice() : null;
      syncColumns(entry);
      entry.table.classList.toggle("ohm-table--fixed", !!widths);
      if (widths) {
        if (!entry.colgroup) {
          entry.colgroup = document.createElement("colgroup");
          entry.columns = widths.map(function () {
            var column = document.createElement("col");
            entry.colgroup.appendChild(column);
            return column;
          });
          entry.table.insertBefore(entry.colgroup, entry.table.firstChild);
        }
        widths.forEach(function (width, index) {
          entry.columns[index].style.width = width + "px";
        });
      } else {
        if (entry.colgroup) entry.table.removeChild(entry.colgroup);
        entry.colgroup = null;
        entry.columns = null;
        entry.table.style.width = entry.originalWidth;
        entry.canvas.style.width = "";
      }
      refreshEntry(entry);
    }

    function writePreferences() {
      var value = Object.keys(preferences).length ? JSON.stringify(preferences) : null;
      if (!storageKey || value === storedValue) return;
      try {
        if (value === null) window.sessionStorage.removeItem(storageKey);
        else window.sessionStorage.setItem(storageKey, value);
        storedValue = value;
      } catch (err) { /* Resizing still works when persistence is unavailable. */ }
    }

    function save(entry) {
      if (entry.widths) preferences[entry.key] = entry.collapsed.some(function (value) { return value; }) ? {
        widths: entry.widths.slice(), collapsed: entry.collapsed.slice(), expanded: entry.expanded.slice(),
      } : entry.widths.slice();
      else delete preferences[entry.key];
      writePreferences();
    }

    function rememberResize(entry) {
      if (!entry.expanded) return;
      entry.expanded = entry.expanded.map(function (width, index) { return entry.collapsed[index] ? width : entry.widths[index]; });
      entry.total = sum(entry.widths);
    }

    function toggleColumn(entry, index) {
      if (active) finishDrag(false);
      if (!entry.collapsed[index] && entry.collapsed.filter(function (value) { return !value; }).length === 1) return;
      var widths = readWidths(entry);
      if (!widths) return;
      if (!entry.expanded) {
        entry.expanded = widths.slice();
        entry.total = sum(widths);
      }
      entry.collapsed[index] = !entry.collapsed[index];
      var hidden = entry.collapsed.filter(function (value) { return value; }).length;
      entry.total = Math.max(entry.total, hidden * collapsedWidth + (widths.length - hidden) * minimum);
      applyWidths(entry, project(entry.expanded, entry.collapsed, entry.total));
      if (!hidden) { entry.expanded = null; entry.total = null; }
      save(entry);
    }

    function different(left, right) {
      return left.some(function (width, index) { return Math.abs(width - right[index]) > 0.001; });
    }

    function minimumTotal(widths, index) {
      return sum(widths) - widths[index] + minimum;
    }

    function resized(widths, index, total) {
      var originalTotal = sum(widths);
      total = Math.max(minimumTotal(widths, index), total);
      return widths.map(function (width, column) {
        return column === index ? Math.max(minimum, total - originalTotal + width) : width;
      });
    }

    function boundary(entry, widths, index) {
      var total = sum(widths) + entry.chrome;
      var bounds = viewport(entry, total);
      var offset = entry.borderLeft + sum(widths.slice(0, index + 1));
      return bounds.left + offset - entry.scroll.scrollLeft;
    }

    function finishDrag(commit) {
      if (!active) return;
      var drag = active;
      active = null;
      window.removeEventListener("pointermove", moveDrag);
      window.removeEventListener("pointerup", endDrag);
      window.removeEventListener("pointercancel", cancelDrag);
      document.removeEventListener("keydown", dragKey);
      drag.handle.removeEventListener("lostpointercapture", cancelDrag);
      document.documentElement.classList.remove("ohm-table-resizing");
      try {
        if (typeof drag.handle.releasePointerCapture === "function") drag.handle.releasePointerCapture(drag.pointerId);
      } catch (err) { /* Capture may already have been lost. */ }
      if (!commit || !drag.entry.widths || !different(drag.widths, drag.entry.widths)) {
        applyWidths(drag.entry, drag.previous);
        drag.entry.scroll.scrollLeft = drag.scrollLeft;
      } else {
        rememberResize(drag.entry);
        save(drag.entry);
      }
      // Keep the initial scroll extent until the final dimensions are chosen.
      // Removing it earlier lets a shrinking canvas clamp scrollLeft between
      // pointer events and repeatedly apply the same apparent pointer delta.
      drag.entry.scroll.removeChild(drag.spacer);
    }

    function cancelDrag(event) {
      if (active && (!event || event.pointerId === undefined || event.pointerId === active.pointerId)) finishDrag(false);
    }

    function dragKey(event) {
      if (event.key === "Escape" && active) {
        event.preventDefault();
        event.stopPropagation();
        finishDrag(false);
      }
    }

    function moveDrag(event) {
      if (!active || (event.pointerId !== undefined && event.pointerId !== active.pointerId) || !Number.isFinite(event.clientX)) return;
      var drag = active;
      if (event.clientX === drag.startX && drag.entry.scroll.scrollLeft === drag.scrollLeft) {
        if (drag.entry.widths) applyWidths(drag.entry, drag.previous);
        return;
      }
      var target = event.clientX - drag.pointerOffset;
      // The centered frame moves left while expanding between the prose and
      // main-pane widths. Invert that monotonic geometry instead of treating
      // clientX delta as a column delta; include current internal scrolling.
      var low = minimumTotal(drag.widths, drag.index);
      var high = Math.max(sum(drag.widths), low + 1);
      function position(total) { return boundary(drag.entry, resized(drag.widths, drag.index, total), drag.index); }
      for (var attempts = 0; position(high) < target && attempts < 40; attempts += 1) high *= 2;
      if (!Number.isFinite(high)) return;
      for (var step = 0; step < 48; step += 1) {
        var middle = (low + high) / 2;
        if (position(middle) < target) low = middle;
        else high = middle;
      }
      var widths = resized(drag.widths, drag.index, (low + high) / 2);
      if (different(drag.widths, widths) || drag.entry.widths) applyWidths(drag.entry, widths);
      event.preventDefault();
    }

    function endDrag(event) {
      if (!active || (event.pointerId !== undefined && event.pointerId !== active.pointerId)) return;
      moveDrag(event);
      finishDrag(true);
    }

    function startDrag(event, entry, index, handle) {
      if (entry.collapsed[index] || active || event.isPrimary === false || (event.button !== undefined && event.button !== 0) || !Number.isFinite(event.clientX)) return;
      measureChrome(entry);
      var widths = readWidths(entry);
      if (!widths) return;
      var spacer = document.createElement("div");
      spacer.className = "ohm-table-drag-spacer";
      spacer.setAttribute("aria-hidden", "true");
      var extent = Math.max(entry.scroll.scrollWidth || 0, entry.table.getBoundingClientRect().width);
      spacer.style.width = extent + "px";
      spacer.style.height = "0px";
      spacer.style.pointerEvents = "none";
      entry.scroll.appendChild(spacer);
      var offset = entry.borderLeft + sum(widths.slice(0, index + 1));
      active = {
        entry: entry, index: index, handle: handle, widths: widths,
        previous: entry.widths ? entry.widths.slice() : null,
        pointerId: event.pointerId,
        pointerOffset: event.clientX - entry.table.getBoundingClientRect().left - offset,
        scrollLeft: entry.scroll.scrollLeft,
        startX: event.clientX, spacer: spacer, extent: extent,
      };
      window.addEventListener("pointermove", moveDrag);
      window.addEventListener("pointerup", endDrag);
      window.addEventListener("pointercancel", cancelDrag);
      document.addEventListener("keydown", dragKey);
      handle.addEventListener("lostpointercapture", cancelDrag);
      document.documentElement.classList.add("ohm-table-resizing");
      if (typeof handle.focus === "function") handle.focus();
      try {
        if (typeof handle.setPointerCapture === "function") handle.setPointerCapture(event.pointerId);
      } catch (err) { /* Window listeners also support uncaptured pointers. */ }
      event.preventDefault();
    }

    function makeHandle(entry, index, label) {
      var handle = document.createElement("span");
      handle.className = "ohm-table-column-handle";
      handle.setAttribute("role", "separator");
      handle.setAttribute("aria-orientation", "vertical");
      handle.setAttribute("tabindex", "0");
      handle.setAttribute("aria-label", label);
      handle.setAttribute("aria-valuemin", String(minimum));
      handle.setAttribute("title", label + "; drag or use Left/Right arrows (Shift for 10 pixels)");
      handle.addEventListener("pointerdown", function (event) { startDrag(event, entry, index, handle); });
      handle.addEventListener("keydown", function (event) {
        if (event.key === "Escape") { dragKey(event); return; }
        if (entry.collapsed[index] || active || (event.key !== "ArrowLeft" && event.key !== "ArrowRight")) return;
        var widths = readWidths(entry);
        if (!widths) return;
        var delta = (event.key === "ArrowLeft" ? -1 : 1) * (event.shiftKey ? 10 : 1);
        var next = resized(widths, index, sum(widths) + delta);
        if (different(widths, next)) { applyWidths(entry, next); rememberResize(entry); save(entry); }
        event.preventDefault();
        event.stopPropagation();
      });
      return handle;
    }

    tables.forEach(function (table) {
      if (table.querySelector("table") || table.querySelector("colgroup") || (table.parentElement && table.parentElement.closest("table"))) return;
      var headerRows = table.querySelectorAll("thead tr");
      var rows = Array.from(table.querySelectorAll("tr"));
      if (headerRows.length !== 1) return;
      var headers = Array.from(headerRows[0].children).filter(function (cell) { return cell.tagName === "TH" || cell.tagName === "TD"; });
      if (!headers.length || !rows.every(function (row) {
        var cells = Array.from(row.children).filter(function (cell) { return cell.tagName === "TH" || cell.tagName === "TD"; });
        return cells.length === headers.length && cells.every(function (cell) {
          return (!cell.hasAttribute("colspan") || cell.getAttribute("colspan") === "1")
            && (!cell.hasAttribute("rowspan") || cell.getAttribute("rowspan") === "1");
        });
      })) return;

      var labels = headers.map(function (header) { return header.textContent.replace(/\s+/g, " ").trim(); });
      var signature = JSON.stringify([headers.length, labels]);
      var occurrence = occurrences.get(signature) || 0;
      occurrences.set(signature, occurrence + 1);
      var parent = table.parentElement;
      var article = table.closest(".ohm-content");
      var originalStyle = computed(table);
      var frame = document.createElement("div");
      frame.className = "ohm-table-frame";
      if (originalStyle.marginTop) frame.style.marginTop = originalStyle.marginTop;
      if (originalStyle.marginBottom) frame.style.marginBottom = originalStyle.marginBottom;
      var scroll = document.createElement("div");
      scroll.className = "ohm-table-scroll";
      var canvas = document.createElement("div");
      canvas.className = "ohm-table-canvas";
      var actions = document.createElement("div");
      actions.className = "ohm-table-actions";
      var reset = document.createElement("button");
      reset.type = "button";
      reset.className = "ohm-icon-button ohm-table-reset";
      reset.setAttribute("aria-label", "Reset widths");
      reset.setAttribute("title", "Reset widths");
      reset.innerHTML = ICONS.reset;
      reset.disabled = true;
      parent.insertBefore(frame, table);
      frame.appendChild(scroll);
      scroll.appendChild(canvas);
      canvas.appendChild(table);
      var footer = Array.from(table.children).find(function (child) { return child.tagName === "TFOOT"; });
      if (!footer) {
        footer = document.createElement("tfoot");
        table.appendChild(footer);
      }
      var controlsRow = document.createElement("tr");
      controlsRow.className = "ohm-table-controls-row";
      var controlsCell = document.createElement("td");
      controlsCell.className = "ohm-table-controls-cell";
      controlsCell.setAttribute("colspan", String(headers.length));
      footer.appendChild(controlsRow);
      controlsRow.appendChild(controlsCell);
      controlsCell.appendChild(actions);
      actions.appendChild(reset);
      var authoredCells = headers.map(function () { return []; });
      rows.forEach(function (row) {
        Array.from(row.children).filter(function (cell) { return cell.tagName === "TH" || cell.tagName === "TD"; }).forEach(function (cell, index) {
          var content = document.createElement("div");
          content.className = "ohm-table-cell-content";
          while (cell.firstChild) content.appendChild(cell.firstChild);
          cell.appendChild(content);
          authoredCells[index].push({ cell: cell, content: content });
        });
      });
      var entry = {
        table: table, headers: headers, frame: frame, scroll: scroll, canvas: canvas, reset: reset,
        article: article, main: article.closest(".ohm-main"), breakout: parent === article,
        originalWidth: table.style.width || "", widths: null, colgroup: null, columns: null,
        chrome: 0, borderLeft: 0, key: JSON.stringify([headers.length, labels, occurrence]),
        labels: labels, cells: authoredCells, collapsed: headers.map(function () { return false; }),
        expanded: null, total: null, toggles: [],
      };
      table.classList.add("ohm-table-resizable");
      table.classList.toggle("ohm-table--collapsible", headers.length > 1);
      entry.handles = headers.map(function (header, index) {
        header.classList.add("ohm-table-column-header");
        var handle = makeHandle(entry, index, "Resize column " + (labels[index] || (index + 1)));
        header.appendChild(handle);
        return handle;
      });
      if (headers.length > 1) entry.toggles = headers.map(function (header, index) {
        var toggle = document.createElement("button");
        toggle.type = "button";
        toggle.className = "ohm-table-column-toggle";
        toggle.innerHTML = ICONS.chevronLeft;
        toggle.addEventListener("click", function (event) {
          event.stopPropagation();
          toggleColumn(entry, index);
        });
        header.appendChild(toggle);
        return toggle;
      });
      reset.addEventListener("click", function () {
        if (active) finishDrag(false);
        entry.collapsed = entry.collapsed.map(function () { return false; });
        entry.expanded = null;
        entry.total = null;
        applyWidths(entry, null);
        entry.scroll.scrollLeft = 0;
        save(entry);
      });
      entries.push(entry);
    });
    if (!entries.length) return;

    var brand = document.querySelector(".ohm-brand[href]");
    if (brand) {
      try {
        var page = new URL(document.URL || window.location.href);
        page.search = "";
        page.hash = "";
        storageKey = "ohimark-table-widths:" + JSON.stringify([brand.href, page.href]);
      } catch (err) { /* Missing page identity disables persistence, not resizing. */ }
    }
    var dirty = false;
    if (storageKey) {
      try {
        storedValue = window.sessionStorage.getItem(storageKey);
        var saved = JSON.parse(storedValue);
        if (saved && typeof saved === "object" && !Array.isArray(saved)) {
          var known = new Map(entries.map(function (entry) { return [entry.key, entry]; }));
          Object.keys(saved).forEach(function (key) {
            var entry = known.get(key);
            if (entry && validLayout(saved[key], entry.headers.length)) preferences[key] = copyLayout(saved[key]);
            else dirty = true;
          });
        } else if (storedValue !== null) dirty = true;
      } catch (err) { dirty = storedValue !== null; }
    }
    entries.forEach(function (entry) {
      var layout = preferences[entry.key];
      if (layout && !Array.isArray(layout)) {
        entry.collapsed = layout.collapsed.slice();
        entry.expanded = layout.expanded.slice();
        entry.total = sum(layout.widths);
      }
      if (layout) applyWidths(entry, Array.isArray(layout) ? layout : layout.widths);
      else { syncColumns(entry); refreshEntry(entry); }
    });
    if (dirty) writePreferences();

    var scheduled = false;
    refreshTableSizes = function () {
      if (scheduled) return;
      scheduled = true;
      function run() {
        scheduled = false;
        entries.forEach(refreshEntry);
      }
      if (typeof window.requestAnimationFrame === "function") window.requestAnimationFrame(run);
      else run();
    };
    window.addEventListener("resize", refreshTableSizes);
    if (navState) navState.addEventListener("change", refreshTableSizes);
    if (outlineState) outlineState.addEventListener("change", refreshTableSizes);
    if (typeof window.ResizeObserver === "function") {
      try {
        var observer = new window.ResizeObserver(refreshTableSizes);
        var observed = new Set();
        entries.forEach(function (entry) {
          [entry.main, entry.article].forEach(function (element) {
            if (element && !observed.has(element)) { observed.add(element); observer.observe(element); }
          });
        });
      } catch (err) { /* Explicit panel, font and viewport refreshes remain available. */ }
    }
    if (document.fonts) {
      if (document.fonts.ready && typeof document.fonts.ready.then === "function") document.fonts.ready.then(refreshTableSizes, function () {});
      if (typeof document.fonts.addEventListener === "function") document.fonts.addEventListener("loadingdone", refreshTableSizes);
    }
  }

  enhanceTables();

  /* =======================================================================
     8. One delegated click listener for every interactive control above,
     plus closing the mobile sidebar drawer on navigation.
     ======================================================================= */

  document.addEventListener("click", function (event) {
    var target = event.target;

    if (navButton && target.closest(".ohm-nav-button") === navButton) {
      toggleNavigation();
      return;
    }

    if (outlineButton && target.closest(".ohm-outline-button") === outlineButton) {
      outlineState.checked = !outlineState.checked;
      writePanelCollapsed("ohimark-outline-collapsed", outlineState.checked);
      syncOutlineButton();
      return;
    }

    if (target.closest("#ohm-theme-button")) {
      applyTheme(currentTheme() === "dark" ? "light" : "dark");
      return;
    }

    var toggleButton = target.closest(".ohm-diagram-toggle");
    if (toggleButton) {
      toggleDiagram(toggleButton);
      return;
    }

    var fullscreenButton = target.closest(".ohm-diagram-fullscreen");
    if (fullscreenButton) {
      toggleDiagramFullscreen(fullscreenButton);
      return;
    }

    var copyButton = target.closest(".ohm-copy-button");
    if (copyButton) {
      handleCopy(copyButton);
      return;
    }

    if (navState && navigationIsMobile() && target.closest(".ohm-sidebar .ohm-nav-link, .ohm-sidebar a.ohm-nav-label")) {
      navState.checked = false;
      syncNavigationButton();
    }
  });
})();
