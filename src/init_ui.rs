//! Built-in initialization screen rendered inside the webview before dsh is up.
//! Styled to match the DeepSeek Harness (DS) web UI dark theme.

pub const INIT_HTML: &str = r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>DSH Desktop</title>
<style>
  :root {
    color-scheme: dark;
    --dsw-font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Helvetica Neue", Helvetica, Arial, sans-serif;
    --bg-base: #151517;
    --bg-surface: #1b1b1c;
    --bg-elevated: #232324;
    --label-primary: #f9fafb;
    --label-secondary: #9aa0a6;
    --border: rgba(255, 255, 255, 0.06);
    --brand: #5686fe;
    --brand-hover: #4176e6;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  html, body { height: 100%; }
  body {
    font-family: var(--dsw-font-family);
    background: var(--bg-base);
    color: var(--label-primary);
    display: flex; align-items: center; justify-content: center;
    -webkit-user-select: none; user-select: none;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }
  .card { width: min(460px, 86vw); display: flex; flex-direction: column; align-items: center; text-align: center; padding: 48px 40px 40px; }
  .logo { width: 56px; height: 56px; margin-bottom: 22px; }
  .logo svg { width: 100%; height: 100%; display: block; }
  .brand h1 { font-size: 18px; font-weight: 600; letter-spacing: 0.2px; }
  .brand .sub { font-size: 12px; color: var(--label-secondary); margin-top: 4px; }
  .spinner {
    width: 20px; height: 20px; margin: 30px auto 22px;
    border: 2px solid rgba(255, 255, 255, 0.1);
    border-top-color: var(--brand); border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  .spinner.hidden { display: none; }
  @keyframes spin { to { transform: rotate(360deg); } }
  #stage-title { font-size: 15px; font-weight: 500; margin-bottom: 10px; }
  #detail {
    font-size: 12.5px; line-height: 1.6; color: var(--label-secondary);
    white-space: pre-wrap; word-break: break-word;
    max-height: 200px; overflow-y: auto; margin-bottom: 4px;
    -webkit-user-select: text; user-select: text; cursor: text;
  }
  #actions { display: flex; gap: 12px; justify-content: center; margin-top: 24px; flex-wrap: wrap; }
  #actions:empty { display: none; }
  button {
    font: inherit; font-size: 13px; font-weight: 500;
    color: #fff; background: var(--brand);
    border: none; border-radius: 8px; padding: 9px 22px; cursor: pointer;
    transition: background 0.15s ease;
  }
  button:hover { background: var(--brand-hover); }
  button.secondary { background: transparent; border: 1px solid var(--border); color: var(--label-primary); }
  button.secondary:hover { background: rgba(255, 255, 255, 0.05); }
</style>
</head>
<body>
  <div class="card">
    <div class="logo">
      <svg viewBox="0 0 50 50" fill="none" xmlns="http://www.w3.org/2000/svg">
        <path d="M48.8354 10.0479C48.3232 9.79199 48.1025 10.2798 47.8032 10.5278C47.7007 10.6079 47.6143 10.7119 47.5273 10.8076C46.7793 11.624 45.9048 12.1597 44.7622 12.0957C43.0923 12 41.666 12.5356 40.4058 13.8398C40.1377 12.2319 39.2476 11.272 37.8926 10.6558C37.1836 10.3359 36.4668 10.0156 35.9702 9.31982C35.6235 8.82373 35.5293 8.27197 35.356 7.72754C35.2456 7.3999 35.1353 7.06396 34.7651 7.00781C34.3633 6.94385 34.2056 7.2876 34.0479 7.57568C33.418 8.75195 33.1733 10.0479 33.1973 11.3599C33.2524 14.312 34.4736 16.6641 36.8999 18.3359C37.1758 18.5278 37.2466 18.7197 37.1597 19C36.9946 19.5757 36.7974 20.1357 36.624 20.7119C36.5137 21.0801 36.3486 21.1597 35.9624 21C34.6309 20.4321 33.481 19.5918 32.4644 18.5757C30.7393 16.8721 29.1792 14.9917 27.2334 13.52C26.7764 13.1758 26.3193 12.856 25.8467 12.5518C23.8618 10.584 26.1069 8.96777 26.627 8.77588C27.1704 8.57568 26.8159 7.8877 25.0591 7.896C23.3022 7.90381 21.6953 8.50391 19.647 9.30371C19.3477 9.42383 19.0322 9.51172 18.7095 9.58398C16.8501 9.22363 14.9199 9.14355 12.9033 9.37598C9.10596 9.80762 6.07275 11.6396 3.84326 14.7681C1.16455 18.5278 0.53418 22.7998 1.30664 27.2559C2.11768 31.9521 4.46582 35.8398 8.07373 38.8799C11.8159 42.0322 16.1255 43.5762 21.041 43.2803C24.0269 43.104 27.3516 42.6963 31.1016 39.4561C32.0469 39.936 33.0396 40.1279 34.686 40.272C35.9546 40.3921 37.1758 40.208 38.1211 40.0078C39.6021 39.688 39.4995 38.2881 38.9639 38.0322C34.623 35.9678 35.5762 36.8081 34.71 36.1279C36.9155 33.4639 40.2402 30.6958 41.54 21.728C41.6426 21.0161 41.5557 20.5679 41.54 19.9917C41.5322 19.6396 41.6108 19.5039 42.0049 19.4639C43.0923 19.3359 44.1479 19.0317 45.1167 18.4878C47.9292 16.9199 49.064 14.3438 49.3315 11.2559C49.3711 10.7837 49.3237 10.2959 48.8354 10.0479ZM24.3262 37.8398C20.1196 34.4639 18.0791 33.3521 17.2358 33.3999C16.4482 33.4482 16.5898 34.3682 16.7632 34.9678C16.9443 35.5601 17.1812 35.9683 17.5117 36.4878C17.7402 36.832 17.8979 37.3442 17.2832 37.728C15.9282 38.584 13.5728 37.4399 13.4624 37.3838C10.7207 35.7358 8.42822 33.5601 6.81348 30.584C5.25342 27.7197 4.34766 24.6479 4.19775 21.3677C4.1582 20.5757 4.38672 20.2959 5.15869 20.1519C6.17529 19.96 7.22314 19.9199 8.23926 20.0718C12.5327 20.7119 16.1885 22.6719 19.2529 25.7759C21.002 27.5439 22.3252 29.6558 23.6885 31.7202C25.1377 33.9121 26.6978 36 28.6831 37.7119C29.3843 38.312 29.9434 38.7681 30.479 39.104C28.8643 39.2881 26.1699 39.3281 24.3262 37.8398ZM26.3433 24.6001C26.3433 24.248 26.6191 23.9678 26.9658 23.9678C27.0444 23.9678 27.1152 23.9839 27.1782 24.0078C27.2651 24.04 27.3438 24.0879 27.4067 24.1602C27.5171 24.272 27.5801 24.4321 27.5801 24.6001C27.5801 24.9521 27.3042 25.2319 26.9575 25.2319C26.6108 25.2319 26.3433 24.9521 26.3433 24.6001ZM32.6064 27.8799C32.2046 28.0479 31.8027 28.1919 31.4165 28.208C30.8179 28.2397 30.1641 27.9922 29.8096 27.688C29.2583 27.2158 28.8643 26.9521 28.6987 26.1279C28.6279 25.7759 28.6675 25.2319 28.7305 24.9199C28.8721 24.248 28.7144 23.8159 28.2495 23.4238C27.8716 23.104 27.3911 23.0161 26.8633 23.0161C26.666 23.0161 26.4849 22.9277 26.3511 22.856C26.1304 22.7441 25.9492 22.4639 26.1226 22.1201C26.1777 22.0078 26.4458 21.7358 26.5088 21.688C27.2256 21.272 28.0527 21.4077 28.8169 21.7197C29.5259 22.0161 30.0615 22.5601 30.834 23.3281C31.6216 24.2559 31.7632 24.5117 32.2124 25.208C32.5669 25.752 32.8901 26.312 33.1104 26.9521C33.2446 27.3521 33.0713 27.6802 32.6064 27.8799Z" fill="#5686fe"/>
      </svg>
    </div>
    <div class="brand">
      <h1>DeepSeek Harness</h1>
      <div class="sub">桌面壳</div>
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
</html>"##;
