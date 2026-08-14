//! Built-in initialization screen rendered inside the webview before dsh is up.

pub const INIT_HTML: &str = r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>DSH Desktop</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  html, body { height: 100%; }
  body {
    font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
    background: radial-gradient(1200px 700px at 50% -10%, #1d2b45 0%, #0d1117 55%, #0a0d12 100%);
    color: #e6edf3;
    display: flex; align-items: center; justify-content: center;
    -webkit-user-select: none; user-select: none;
  }
  .card {
    width: min(560px, 86vw);
    background: rgba(22, 27, 34, 0.72);
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: 18px;
    padding: 44px 40px 36px;
    box-shadow: 0 24px 70px rgba(0,0,0,0.45);
    backdrop-filter: blur(10px);
  }
  .brand { display: flex; align-items: center; gap: 14px; margin-bottom: 30px; }
  .logo {
    width: 44px; height: 44px; border-radius: 12px;
    background: linear-gradient(135deg, #4d9fff, #7c5cff);
    display: flex; align-items: center; justify-content: center;
    font-size: 22px; font-weight: 700; color: #fff;
  }
  .brand h1 { font-size: 20px; font-weight: 600; letter-spacing: 0.2px; }
  .brand .sub { font-size: 12px; color: #8b949e; margin-top: 2px; }
  .spinner {
    width: 18px; height: 18px; margin: 0 auto 22px;
    border: 2px solid rgba(255,255,255,0.15);
    border-top-color: #4d9fff; border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  .spinner.hidden { display: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
  #stage-title { font-size: 17px; font-weight: 600; text-align: center; margin-bottom: 14px; }
  #detail {
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 12.5px; line-height: 1.55; color: #9aa7b4;
    white-space: pre-wrap; word-break: break-word;
    max-height: 220px; overflow-y: auto;
    margin-bottom: 6px;
    -webkit-user-select: text; user-select: text; cursor: text;
  }
  #actions { display: flex; gap: 12px; justify-content: center; margin-top: 24px; flex-wrap: wrap; }
  #actions:empty { display: none; }
  button {
    font: inherit; font-size: 14px; font-weight: 500;
    color: #fff; background: linear-gradient(135deg, #3b82f6, #6366f1);
    border: none; border-radius: 10px; padding: 10px 22px; cursor: pointer;
    transition: transform 0.06s ease, filter 0.12s ease;
  }
  button:hover { filter: brightness(1.1); }
  button:active { transform: translateY(1px); }
  button.secondary {
    background: rgba(255,255,255,0.07);
    border: 1px solid rgba(255,255,255,0.14);
  }
</style>
</head>
<body>
  <div class="card">
    <div class="brand">
      <div class="logo">D</div>
      <div>
        <h1>DSH Desktop</h1>
        <div class="sub">DeepSeek Harness 桌面壳</div>
      </div>
    </div>
    <div class="spinner" id="spinner"></div>
    <div id="stage-title">正在初始化…</div>
    <div id="detail"></div>
    <div id="actions"></div>
  </div>
<script>
  var LABELS = { retry: "重试", upgrade: "立即升级", skip: "跳过，先用当前版本" };
  window.__dshSetStatus = function (s) {
    document.getElementById("stage-title").textContent = s.title || "";
    document.getElementById("detail").textContent = s.detail || "";
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
</html>"#;
