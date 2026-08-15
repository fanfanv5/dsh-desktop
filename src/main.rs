#![cfg_attr(windows, windows_subsystem = "windows")]

mod borderless;
mod events;
mod init_ui;
mod lifecycle;
mod node;
mod paths;
mod process;
mod runtime;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Icon, WindowBuilder};
use wry::WebViewBuilder;

use events::{Command, UiEvent};
use process::{Job, ProcessState};

fn main() {
    // Escape endpoint-security-injected process trees. The machine's security
    // agents (EsaFeNet DocGuard / Ronds EDR) mark some launch chains with an
    // EFC_* env var, and inside marked trees junction reads/creates fail with
    // "filename syntax incorrect" — which breaks both dsh's profile heal and
    // its loader resolution (99 packages unresolvable). Re-launching through
    // Explorer gives a fresh parent chain without the marker. A marker file
    // fresh within 60s prevents a relaunch loop when the escape doesn't help.
    #[cfg(windows)]
    {
        let marked = std::env::vars().any(|(k, _)| k.starts_with("EFC_"));
        if marked {
            let guard = paths::AppPaths::resolve().logs.join(".relaunched");
            let recent = std::fs::metadata(&guard)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|e| e.as_secs() < 60)
                .unwrap_or(false);
            if !recent {
                let _ = std::fs::write(&guard, b"");
                if let Ok(exe) = std::env::current_exe() {
                    if std::process::Command::new("explorer.exe").arg(&exe).spawn().is_ok() {
                        std::process::exit(0);
                    }
                }
            }
        }
    }

    // Single instance: two concurrent instances race their npm installs and
    // profile heals, corrupting the shared runtime/profile state.
    #[cfg(windows)]
    {
        use std::thread;
        use std::time::Duration;
        use windows::core::w;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        // A just-closed instance still holds the mutex for a moment while
        // WebView2 tears down; instead of exiting silently (which looks like
        // "double-click does nothing"), wait briefly for it to die.
        const GRACE_POLLS: u32 = 15;
        for poll in 0..GRACE_POLLS {
            unsafe {
                if let Ok(handle) = CreateMutexW(None, false, w!("DSHDesktop.SingleInstance")) {
                    if GetLastError() == ERROR_ALREADY_EXISTS {
                        let _ = handle;
                        if poll + 1 < GRACE_POLLS {
                            thread::sleep(Duration::from_millis(200));
                            continue;
                        }
                        std::process::exit(0);
                    }
                    // Intentionally leak the handle: process exit releases the mutex.
                }
            }
            break;
        }
    }

    // Single instance (Unix): an exclusive flock on a lock file in the app
    // data dir — same purpose as the Windows mutex above. The fd is
    // intentionally leaked; the OS drops the lock at process exit, including
    // on crash. Best-effort: if the lock file can't be opened, continue.
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let lock = paths::AppPaths::resolve().logs.join(".instance.lock");
        // First run: the dir doesn't exist yet; create it so the lock works.
        let _ = std::fs::create_dir_all(lock.parent().unwrap_or(std::path::Path::new(".")));
        if let Ok(f) = std::fs::OpenOptions::new().create(true).write(true).open(&lock) {
            if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                std::process::exit(0);
            }
            std::mem::forget(f);
        }
    }

    let paths = paths::AppPaths::resolve();
    let _ = paths.ensure();

    let job = Arc::new(Job::create().expect("failed to create job object"));
    let state = Arc::new(ProcessState::new());

    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // macOS: an app without a main menu gets no key equivalents — WKWebView's
    // Cmd+C/V/Z/A ride on the menu system (performKeyEquivalent → validate),
    // so a bare tao window can't even copy/paste. Install a minimal native
    // menu of predefined items; the webview is the only "document", so no
    // custom actions are needed. The Menu is leaked: NSApp retains the NSMenu,
    // but keeping the Rust wrapper alive is harmless and avoids relying on
    // platform-specific drop semantics.
    #[cfg(target_os = "macos")]
    {
        use muda::{Menu, PredefinedMenuItem, Submenu};
        let app_submenu = Submenu::new("DSH Desktop", true);
        let _ = app_submenu.append_items(&[
            &PredefinedMenuItem::about(Some("DSH Desktop"), None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ]);
        let edit_submenu = Submenu::new("Edit", true);
        let _ = edit_submenu.append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ]);
        let window_submenu = Submenu::new("Window", true);
        let _ = window_submenu.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
        ]);
        let menu = Menu::new();
        let _ = menu.append_items(&[&app_submenu, &edit_submenu, &window_submenu]);
        menu.init_for_nsapp();
        std::mem::forget(menu);
    }

    // Start hidden to avoid a white flash while WebView2 initializes; the
    // window is revealed after the first presented frame (see below).
    // `mut` only for the macOS chrome chaining below; on other targets the
    // builder is never reassigned.
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut builder = WindowBuilder::new()
        .with_visible(false)
        // Same product name the dsh web UI shows; avoids an obvious title
        // change when the webview loads.
        .with_title("DeepSeek Harness")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(720.0, 520.0))
        .with_window_icon(load_icon());
    // macOS: transparent titlebar with fullsize content view — the content
    // extends under the (still native) traffic lights, like VS Code/Telegram.
    // The injected header strip provides the drag surface; the traffic lights
    // themselves stay native, so no in-page window buttons are needed there.
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::WindowBuilderExtMacOS;
        builder = builder
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true);
    }
    let window = builder.build(&event_loop).expect("failed to build window");

    // Paint the native chrome in the content's colors BEFORE anything is
    // shown: at double-click launch the frame and the page must already match.
    #[cfg(windows)]
    let dark = system_prefers_dark();
    #[cfg(target_os = "macos")]
    let dark = system_prefers_dark();
    #[cfg(not(any(windows, target_os = "macos")))]
    let dark = false;
    #[cfg(windows)]
    apply_window_chrome(&window, dark);
    // Borderless: the in-page header bar (injected below) replaces the
    // native caption; resize/snap/maximize stay native via the subclass.
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        use windows::Win32::Foundation::HWND;
        borderless::apply(HWND(window.hwnd() as *mut _));
    }
    // dsh's bg-base token: #f9fafb light / #151517 dark.
    let webview_bg: wry::RGBA = if dark { (21, 21, 23, 255) } else { (249, 250, 251, 255) };

    let webview = WebViewBuilder::new()
        .with_html(init_ui::INIT_HTML)
        // Matches the init screen AND the dsh UI background, so the gap
        // between navigations never flashes a foreign color.
        .with_background_color(webview_bg)
        // Reveal the window only after the first frame is presented (double
        // rAF after DOMContentLoaded), so the user never sees a blank window.
        // The same script also bridges theme changes to the native chrome and
        // injects the in-page header bar (window controls) on every page.
        .with_initialization_script(
            r#"
// Platform token substituted by Rust at startup: 'mac' or 'win'. On macOS
// the window keeps its native traffic lights (over a transparent titlebar),
// so the injected header is a drag-only strip; on Windows it carries the
// min/max/close buttons because the native caption is removed.
var IS_MAC = '@@PLATFORM@@' === 'mac';
var HDR_H = IS_MAC ? 28 : 36;
window.addEventListener('DOMContentLoaded', () => requestAnimationFrame(() => requestAnimationFrame(() => { try { window.ipc.postMessage('__visible'); } catch (e) {} })));
(function () {
  function ipc(m) { try { window.ipc.postMessage(m); } catch (e) {} }
  window.__dshDesktop = {
    minimize: function () { ipc('__win:min'); },
    toggleMaximize: function () { ipc('__win:max'); },
    close: function () { ipc('__win:close'); },
    startDrag: function () { ipc('__win:drag'); }
  };
  var reported;
  function reportTheme() {
    try { window.ipc.postMessage(document.body.hasAttribute('data-ds-dark-theme') ? '__theme:dark' : '__theme:light'); } catch (e) {}
  }
  function startTheme() {
    if (document.body && !reported) {
      reported = true;
      new MutationObserver(reportTheme).observe(document.body, { attributes: true, attributeFilter: ['data-ds-dark-theme'] });
      reportTheme();
    }
  }
  var headerDone = false, headerBar = null;
  function updateHeaderColor() {
    if (!headerBar) return;
    // macOS: the strip is fully transparent everywhere — both the sidebar
    // and the content column are padded down (not margined), so their own
    // backgrounds extend to the top of the window and show through the
    // strip. Colors match by construction; nothing to guess.
    if (IS_MAC) {
      headerBar.style.background = 'transparent';
      return;
    }
    // dsw static palette is stable across pages/themes; the alias vars get
    // redefined by the dsh theme layer at runtime.
    var dark = document.body && document.body.hasAttribute('data-ds-dark-theme');
    if (headerBar.dataset.mode === 'content') {
      // Over the dsh content column: match its background (white / dark
      // elevated), not the sidebar tone.
      headerBar.style.background = dark
        ? 'var(--dsw-static-neutral-bluish-875, #232324)'
        : 'var(--dsw-static-neutral-bluish-00, #ffffff)';
    } else {
      headerBar.style.background = dark
        ? 'var(--dsw-static-neutral-bluish-950, var(--bg-base, #151517))'
        : 'var(--dsw-static-neutral-bluish-50, var(--bg-base, #f9fafb))';
    }
  }
  function buildStrip() {
    headerBar = document.createElement('div');
    headerBar.id = 'dsh-desktop-header';
    // No title: the dsh sidebar carries its own logo at the top; the bar is
    // purely a drag surface + (Windows only) window controls.
    headerBar.setAttribute('style',
      'position:fixed;top:0;left:' + (IS_MAC ? '78' : '0') + 'px;right:0;height:' + HDR_H + 'px;z-index:2147483647;display:flex;align-items:stretch;justify-content:flex-end' + (IS_MAC ? ';cursor:grab' : ''));
    if (!IS_MAC) {
      var right = document.createElement('div');
      right.setAttribute('style', 'display:flex;height:100%');
      var iconMin = '<svg width="10" height="10" viewBox="0 0 10 10"><rect y="4.5" width="10" height="1" fill="currentColor"/></svg>';
      var iconMax = '<svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor"/></svg>';
      var iconClose = '<svg width="10" height="10" viewBox="0 0 10 10"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" stroke-width="1.1"/></svg>';
      function makeButton(icon, action) {
        var b = document.createElement('div');
        b.className = 'dsh-winbtn';
        b.innerHTML = icon;
        b.setAttribute('style', 'width:46px;height:100%;display:flex;align-items:center;justify-content:center;cursor:default;color:var(--dsw-alias-label-secondary,var(--label-secondary,#61666b))');
        var close = action === 'close';
        b.addEventListener('mouseenter', function () {
          b.style.background = close ? '#e81123' : 'rgba(128,128,128,.15)';
          if (close) b.style.color = '#fff';
        });
        b.addEventListener('mouseleave', function () {
          b.style.background = 'transparent';
          b.style.color = 'var(--dsw-alias-label-secondary,var(--label-secondary,#61666b))';
        });
        b.addEventListener('click', function (e) { e.stopPropagation(); window.__dshDesktop[action](); });
        return b;
      }
      right.appendChild(makeButton(iconMin, 'minimize'));
      right.appendChild(makeButton(iconMax, 'toggleMaximize'));
      right.appendChild(makeButton(iconClose, 'close'));
      headerBar.appendChild(right);
    } else {
      // macOS: native traffic lights already sit at the top-left, above the
      // webview; double-click anywhere on the strip zooms, like a real titlebar.
      headerBar.addEventListener('dblclick', function (e) {
        if (e.button === 0) window.__dshDesktop.toggleMaximize();
      });
    }
    // Drag anywhere on the bar (buttons excepted); the native NC drag loop
    // also gives double-click-to-maximize and Aero snap for free.
    headerBar.addEventListener('mousedown', function (e) {
      if (e.button === 0 && !e.target.closest('.dsh-winbtn')) {
        if (IS_MAC) {
          headerBar.style.cursor = 'grabbing';
          window.addEventListener('mouseup', function reset() {
            headerBar.style.cursor = 'grab';
            window.removeEventListener('mouseup', reset);
          });
        }
        window.__dshDesktop.startDrag();
      }
    });
  }
  function installHeader() {
    if (headerDone || !document.body) return;
    headerDone = true;
    if (window.__dshSetStatus) {
      // Our own init page: full-width bar, push the whole page down.
      buildStrip();
      document.body.appendChild(headerBar);
      updateHeaderColor();
      document.documentElement.style.boxSizing = 'border-box';
      document.documentElement.style.paddingTop = HDR_H + 'px';
      return;
    }
    // dsh page: the sidebar goes full-height (its logo reaches the very
    // top); only the content column is pushed below the header bar, which
    // floats over the content area only — its left edge tracks the sidebar
    // width (resize/collapse included, via ResizeObserver).
    var tries = 0;
    (function findFrame() {
      var sidebar = document.querySelector('[data-slot="sidebar"]');
      var sidebarCol = sidebar && sidebar.parentElement;
      var frame = sidebarCol && sidebarCol.parentElement;
      var main = null;
      if (frame) {
        for (var i = 0; i < frame.children.length; i++) {
          var c = frame.children[i];
          if (c !== sidebarCol && c.offsetWidth > 100) { main = c; break; }
        }
      }
      if (!main) {
        if (++tries < 50) return setTimeout(findFrame, 100);
        // Layout hook not found (dsh update?): fall back to full-width bar.
        buildStrip();
        document.body.appendChild(headerBar);
        updateHeaderColor();
        document.documentElement.style.boxSizing = 'border-box';
        document.documentElement.style.paddingTop = HDR_H + 'px';
        return;
      }
      if (IS_MAC) {
        // macOS: only the sidebar is padded down (its top-left sits under the
        // native traffic lights); the content column runs to the very top —
        // no dead space. The drag strip is transparent (updateHeaderColor),
        // so the colors seen through it are always the page's own.
        sidebarCol.style.boxSizing = 'border-box';
        sidebarCol.style.paddingTop = HDR_H + 'px';
        buildStrip();
        document.body.appendChild(headerBar);
        updateHeaderColor();
        return;
      }
      main.style.marginTop = HDR_H + 'px';
      buildStrip();
      headerBar.dataset.mode = 'content';
      var syncLeft = function () {
        headerBar.style.left = Math.round(sidebarCol.getBoundingClientRect().right) + 'px';
      };
      if (window.ResizeObserver) new ResizeObserver(syncLeft).observe(sidebarCol);
      window.addEventListener('resize', syncLeft);
      syncLeft();
      document.body.appendChild(headerBar);
      updateHeaderColor();
    })();
  }
  function startTheme() {
    if (document.body && !reported) {
      reported = true;
      new MutationObserver(function () { reportTheme(); updateHeaderColor(); })
        .observe(document.body, { attributes: true, attributeFilter: ['data-ds-dark-theme'] });
      reportTheme();
    }
  }
  document.addEventListener('DOMContentLoaded', function () { startTheme(); installHeader(); });
  setTimeout(function () { startTheme(); installHeader(); }, 0);
})();
"#
            .replace("@@PLATFORM@@", if cfg!(target_os = "macos") { "mac" } else { "win" }),
        )
        .with_ipc_handler({
            let proxy = proxy.clone();
            move |req| {
                let _ = proxy.send_event(UiEvent::Ipc(req.body().to_string()));
            }
        })
        .build(&window)
        .expect("failed to build webview");

    let cmd_tx = lifecycle::start(proxy.clone(), job.clone(), state.clone());

    // Push an initial status so the init screen is never blank.
    send_status(&webview, "正在初始化…", "", &[]);

    // Deduplicates the two reveal paths (DOMContentLoaded ipc + first status);
    // the closure below is FnMut, so a plain bool capture works.
    let mut revealed = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UiEvent::Status { title, detail, actions }) => {
                // Fallback reveal in case DOMContentLoaded never fires
                // (e.g. WebView2 init failure), so the window is never
                // permanently invisible.
                if !revealed {
                    revealed = true;
                    let _ = window.set_visible(true);
                }
                send_status(&webview, &title, &detail, &actions);
            }
            Event::UserEvent(UiEvent::Ready { url }) => {
                let _ = webview.load_url(&url);
            }
            Event::UserEvent(UiEvent::Ipc(body)) => {
                // Internal pings must be checked first so they are never
                // mistaken for commands. __theme fires on every page load and
                // whenever the user switches the theme inside the dsh UI —
                // the native chrome follows the page. __win:* come from the
                // injected header bar (the window has no native caption).
                if body.contains("__theme:dark") {
                    #[cfg(windows)]
                    apply_window_chrome(&window, true);
                } else if body.contains("__theme:light") {
                    #[cfg(windows)]
                    apply_window_chrome(&window, false);
                } else if body.contains("__win:min") {
                    let _ = window.set_minimized(true);
                } else if body.contains("__win:max") {
                    let _ = window.set_maximized(!window.is_maximized());
                } else if body.contains("__win:drag") {
                    #[cfg(windows)]
                    {
                        use tao::platform::windows::WindowExtWindows;
                        use windows::Win32::Foundation::HWND;
                        borderless::begin_drag(HWND(window.hwnd() as *mut _));
                    }
                    // macOS: tao synthesizes a left-mouse-down event from the
                    // current event and enters the native drag loop.
                    #[cfg(not(windows))]
                    {
                        let _ = window.drag_window();
                    }
                } else if body.contains("__win:close") {
                    // Same path as CloseRequested: hide first, then teardown.
                    let _ = window.set_visible(false);
                    cleanup(&job, &state);
                    *control_flow = ControlFlow::Exit;
                } else if body.contains("__visible") {
                    if !revealed {
                        revealed = true;
                        let _ = window.set_visible(true);
                    }
                } else {
                    let cmd = if body.contains("upgrade") {
                        Some(Command::Upgrade)
                    } else if body.contains("skip") {
                        Some(Command::SkipUpdate)
                    } else if body.contains("retry") {
                        Some(Command::Retry)
                    } else {
                        None
                    };
                    if let Some(cmd) = cmd {
                        let _ = cmd_tx.send(cmd);
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                // Hide first so teardown cost is invisible to the user.
                let _ = window.set_visible(false);
                cleanup(&job, &state);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

/// The system's apps color scheme on macOS: `defaults read -g
/// AppleInterfaceStyle` prints "Dark" in dark mode and fails (key absent) in
/// light mode. Only used to pick the webview's pre-paint background color so
/// the first frame matches the page; WKWebView itself follows the system.
#[cfg(target_os = "macos")]
fn system_prefers_dark() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "Dark")
        .unwrap_or(false)
}

/// The system's apps color scheme (the same signal Chromium/WebView2 uses for
/// prefers-color-scheme): HKCU\...\Themes\Personalize!AppsUseLightTheme.
/// Defaults to light when the value can't be read.
#[cfg(windows)]
fn system_prefers_dark() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        REG_VALUE_TYPE,
    };
    const SUBKEY: PCWSTR = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    const VALUE: PCWSTR = windows::core::w!("AppsUseLightTheme");
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, SUBKEY, None, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let mut data: u32 = 1;
        let mut ty = REG_VALUE_TYPE::default();
        let mut size = 4u32;
        let ok = RegQueryValueExW(
            hkey,
            VALUE,
            None,
            Some(&mut ty),
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut size),
        )
        .is_ok();
        let _ = RegCloseKey(hkey);
        ok && data == 0
    }
}

/// Paint the native window chrome (caption, border, title text) in the same
/// colors as the web content, so the system frame doesn't read as a separate
/// shell bolted onto the app. Called when the init page reports the system
/// theme — the same signal dsh itself uses to pick its palette.
#[cfg(windows)]
fn apply_window_chrome(window: &tao::window::Window, dark: bool) {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
    };

    // dsh design tokens (COLORREF = 0x00BBGGRR):
    // bg-base #f9fafb / #151517, label-primary #0f1115 / #f9fafb.
    let (bg, text) = if dark {
        (0x0017_1515u32, 0x00FB_FAF9u32)
    } else {
        (0x00FB_FAF9u32, 0x0015_110Fu32)
    };
    let hwnd = HWND(window.hwnd() as *mut _);
    unsafe {
        let immersive: i32 = dark as i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &immersive as *const _ as *const _,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &bg as *const _ as *const _,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &bg as *const _ as *const _,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text as *const _ as *const _,
            4,
        );
    }
}

fn cleanup(job: &Job, state: &ProcessState) {    if let Some(pid) = *state.pid.lock().unwrap() {
        if state.assigned.load(Ordering::Relaxed) {
            job.kill_all(pid);
        } else {
            // Fallback for when the child could not be placed in the job object.
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                // CREATE_NO_WINDOW: GUI app, a bare taskkill would flash a console.
                let _ = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .creation_flags(0x0800_0000)
                    .output();
            }
            #[cfg(unix)]
            {
                unsafe { let _ = libc::kill(pid as i32, libc::SIGKILL); }
            }
        }
    }
}

/// Load the window/titlebar icon from the embedded RGBA data.
fn load_icon() -> Option<Icon> {
    const ICON_RGBA: &[u8] = include_bytes!("../assets/icon-64.rgba");
    const ICON_SIZE: u32 = 64;
    Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE).ok()
}

fn send_status(webview: &wry::WebView, title: &str, detail: &str, actions: &[String]) {
    let json = format!(
        "{{title:{},detail:{},actions:[{}]}}",
        js_str(title),
        js_str(detail),
        actions.iter().map(|a| js_str(a)).collect::<Vec<_>>().join(",")
    );
    let _ = webview.evaluate_script(&format!("window.__dshSetStatus({})", json));
}

fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
