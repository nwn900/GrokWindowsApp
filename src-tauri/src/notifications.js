(() => {
  "use strict";
  if (window.top !== window || window.__desktopWatcher) return;
  const configs = {
    "chatgpt.com": ['[data-message-author-role="assistant"]', 'button[data-testid="stop-button"],button[aria-label*="Stop"]'],
    "gemini.google.com": ['model-response', 'button[aria-label*="Stop"],.stop-button'],
    "claude.ai": ['[data-is-streaming],.font-claude-response', 'button[aria-label*="Stop"],[data-is-streaming="true"]'],
    "grok.com": ['.response-content-markdown,[data-message-author-role="assistant"]', 'button[aria-label*="Stop"],button[data-testid="stop-button"]']
  };
  const config = configs[location.hostname];
  if (!config) return;
  window.__desktopWatcher = true;
  let baseline = null, active = false, changedAt = 0, timer = 0, route = location.pathname;
  let pending = false, hasChanged = false;
  async function scan() {
    timer = 0;
    const nodes = document.querySelectorAll(config[0]);
    const last = nodes[nodes.length - 1];
    // textContent does not force layout, unlike innerText.
    const text = last ? (last.textContent || "").trim() : "";
    const busy = !!document.querySelector(config[1]);
    if (route !== location.pathname || baseline === null) {
      route = location.pathname; baseline = text; active = busy; hasChanged = false; changedAt = Date.now();
      return;
    }
    if (text !== baseline) { if (active || busy) hasChanged = true; changedAt = Date.now(); baseline = text; }
    if (busy) { active = true; changedAt = Date.now(); }
    if (active && hasChanged && !busy && text && Date.now() - changedAt >= 2500 && !pending) {
      const invoke = window.__TAURI__?.core?.invoke;
      if (!invoke) { schedule(); return; }
      pending = true;
      try {
        await invoke("notify_answer", { body: text.slice(0, 300) });
        active = false; hasChanged = false;
      } catch (error) { console.debug("Desktop notification:", error); }
      finally { pending = false; }
    }
    if (active) schedule();
  }
  function schedule() { if (!timer) timer = setTimeout(scan, 500); }
  function start() {
    document.addEventListener("submit", () => { active = true; changedAt = Date.now(); schedule(); }, true);
    document.addEventListener("keydown", event => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing &&
          event.target?.matches?.("textarea,[contenteditable=true]")) {
        active = true; changedAt = Date.now(); schedule();
      }
    }, true);
    new MutationObserver(schedule).observe(document.documentElement,
      { subtree: true, childList: true, characterData: true, attributes: true,
        attributeFilter: ["aria-label", "data-is-streaming"] });
    scan();
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
  else start();
})();
