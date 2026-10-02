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
    figure.appendChild(actions);
  });

  function toggleDiagram(button) {
    var figure = button.closest(".ohm-diagram");
    if (!figure) return;
    var showingSource = figure.classList.toggle("ohm-diagram--source");
    button.setAttribute("aria-expanded", showingSource ? "true" : "false");
    button.setAttribute("aria-label", showingSource ? "Show diagram" : "Show diagram source");
  }

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
