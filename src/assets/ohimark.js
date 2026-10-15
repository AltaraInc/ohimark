/*
 * ohimark.js — progressive-enhancement script for every ohimark-built site.
 *
 * Implements: R8.1 (theme toggle button, kept in sync with the inline head
 * script that already applied the stored/system theme before paint), R8.6
 * (per-diagram source/graphic toggle), R8.10 (copy-to-clipboard on code
 * blocks and diagrams), plus outline scroll-highlighting and collapsible
 * sidebars with a mobile navigation drawer.
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

  if (outlineLinks.length && "IntersectionObserver" in window) {
    var itemByHeadingId = {};
    var headings = [];

    for (var j = 0; j < outlineLinks.length; j++) {
      var href = outlineLinks[j].getAttribute("href") || "";
      var id = href.slice(1);
      if (!id) {
        continue;
      }
      var heading = document.getElementById(id);
      if (!heading) {
        continue;
      }
      itemByHeadingId[id] = outlineLinks[j].parentElement;
      headings.push(heading);
    }

    var setActive = function (item) {
      var current = document.querySelector(".ohm-outline-item--active");
      if (current && current !== item) {
        current.classList.remove("ohm-outline-item--active");
      }
      if (item) {
        item.classList.add("ohm-outline-item--active");
      }
    };

    var observer = new IntersectionObserver(
      function (entries) {
        for (var k = 0; k < entries.length; k++) {
          var entry = entries[k];
          if (!entry.isIntersecting) {
            continue;
          }
          var item = itemByHeadingId[entry.target.id];
          if (item) {
            setActive(item);
          }
        }
      },
      { rootMargin: "0px 0px -70% 0px", threshold: 0 }
    );

    for (var m = 0; m < headings.length; m++) {
      observer.observe(headings[m]);
    }
  }

  /* =======================================================================
     5. Sidebar controls: preserve independent preferences while keeping
     the mobile navigation drawer closed when entering the mobile layout.
     ======================================================================= */

  var navState = document.getElementById("ohm-nav-state");
  var outlineState = document.getElementById("ohm-outline-state");
  var navButton = null;
  var outlineButton = null;
  var navMedia = null;
  var desktopSidebarCollapsed = false;
  var navigationWasMobile = false;

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

  /* =======================================================================
     6. One delegated click listener for every interactive control above,
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
