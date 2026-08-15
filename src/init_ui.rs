//! Built-in initialization screen rendered inside the webview before dsh is up.
//!
//! Mirrors dsh's own frontend boot state (dsh-web-frontend `_boot_*` styles):
//! same system-following theme detection, same design tokens, same centered
//! card + wordmark + spinner. The goal is an invisible transition — the init
//! screen should look exactly like the moment before dsh paints its UI.

pub const INIT_HTML: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>DeepSeek Harness</title>
<style>
  /* Tokens copied from dsh-web-frontend's design system (dsw alias palette). */
  :root {
    color-scheme: light;
    --dsw-font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Helvetica Neue", Helvetica, Arial, sans-serif;
    --ds-font-family-code: ui-monospace, "SF Mono", Menlo, Consolas, "Courier New";
    --bg-base: #f9fafb;
    --label-primary: #0f1115;
    --label-secondary: #61666b;
    --label-tertiary: #81858c;
    --border-l2: rgba(0, 0, 0, .1);
    --brand: #3964fe;
    --brand-hover: #2f55e0;
  }
  body[data-ds-dark-theme] {
    --bg-base: #151517;
    --label-primary: #f9fafb;
    --label-secondary: #cfcfd6;
    --label-tertiary: #adb2b8;
    --border-l2: rgba(255, 255, 255, .12);
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  html, body { height: 100%; }
  body {
    font-family: var(--dsw-font-family);
    background: var(--bg-base);
    color: var(--label-primary);
    display: grid; place-items: center;
    -webkit-user-select: none; user-select: none;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }
  /* Identical structure to dsh's own boot screen (_boot_/_card_/_wordmark_):
     text wordmark, spinner, hint. When dsh loads, its boot screen looks the
     same, so the handoff is invisible — one continuous loading experience. */
  .card { display: flex; flex-direction: column; align-items: center; gap: 16px; }
  .wordmark {
    font-size: 16px; line-height: 24px; font-weight: 600;
    letter-spacing: .08em; color: var(--label-primary);
  }
  .spinner {
    width: 20px; height: 20px; border-radius: 50%;
    border: 2px solid var(--border-l2);
    border-top-color: var(--brand);
    animation: spin .8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  /* Loading state: hint line under the spinner (dsh shows "Loading plugins…"). */
  #stage-title {
    font-size: 12px; line-height: 18px; color: var(--label-tertiary);
    text-align: center; max-width: 480px;
  }
  /* Failure state mirrors dsh's _failedTitle_/_failedItem_ classes. */
  .failed #stage-title {
    font-size: 14px; line-height: 22px; font-weight: 600;
    color: var(--label-primary); font-family: var(--dsw-font-family);
  }
  #detail {
    font-family: var(--ds-font-family-code);
    font-size: 12px; line-height: 18px; color: var(--label-secondary);
    white-space: pre-wrap; word-break: break-word; text-align: left;
    max-width: 480px; max-height: 200px; overflow-y: auto;
    -webkit-user-select: text; user-select: text; cursor: text;
  }
  #actions { display: flex; gap: 12px; justify-content: center; margin-top: 4px; flex-wrap: wrap; }
  #actions:empty { display: none; }
  button {
    font: inherit; font-size: 13px; font-weight: 500;
    color: #fff; background: var(--brand);
    border: none; border-radius: 8px; padding: 9px 22px; cursor: pointer;
    transition: background .15s ease;
  }
  button:hover { background: var(--brand-hover); }
  button.secondary {
    background: transparent; border: 1px solid var(--border-l2);
    color: var(--label-primary);
  }
  button.secondary:hover { background: var(--border-l2); }
</style>
</head>
<body>
  <div class="card" id="card">
    <div class="wordmark">HARNESS</div>
    <div class="spinner" id="spinner"></div>
    <div id="stage-title">Starting…</div>
    <div id="detail"></div>
    <div id="actions"></div>
  </div>
<script>
  // Same theme detection as dsh's index.html: follow the system preference so
  // the init screen always matches the theme dsh itself will render with.
  // Rust reads the same system setting for the native window chrome.
  (function () {
    var dark = window.matchMedia
      && window.matchMedia("(prefers-color-scheme: dark)").matches;
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
    document.body.toggleAttribute("data-ds-dark-theme", dark);
  })();

  var LABELS = { retry: "重试", upgrade: "立即升级", skip: "跳过，先用当前版本" };
  window.__dshSetStatus = function (s) {
    document.getElementById("stage-title").textContent = s.title || "";
    document.getElementById("detail").textContent = s.detail || "";
    // dsh's boot screen pairs spinner+hint while loading and switches to a
    // title+items block once failed; mirror that by the presence of actions.
    var failed = (s.actions || []).length > 0;
    document.getElementById("card").classList.toggle("failed", failed);
    document.getElementById("spinner").style.display = failed ? "none" : "";
    var box = document.getElementById("actions");
    box.innerHTML = "";
    (s.actions || []).forEach(function (key) {
      var b = document.createElement("button");
      b.textContent = LABELS[key] || key;
      b.className = (key === "skip") ? "secondary" : "";
      b.onclick = function () {
        try { window.ipc.postMessage(JSON.stringify({ action: key })); } catch (e) {}
      };
      box.appendChild(b);
    });
    if (s.detail) {
      var c = document.createElement("button");
      c.textContent = "复制错误详情";
      c.className = "secondary";
      c.onclick = function () {
        var text = document.getElementById("detail").textContent || "";
        var done = function () { c.textContent = "已复制"; };
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(done, function () { fallbackCopy(text, c, done); });
        } else {
          fallbackCopy(text, c, done);
        }
      };
      box.appendChild(c);
    }
  };
  function fallbackCopy(text, btn, done) {
    try {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.focus();
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
      done();
    } catch (e) {
      btn.textContent = "请手动选中复制";
    }
  }
</script>
</body>
</html>"##;
