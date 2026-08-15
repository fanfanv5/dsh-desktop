#![cfg_attr(windows, windows_subsystem = "windows")]

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
        use windows::core::w;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;
        unsafe {
            if CreateMutexW(None, false, w!("DSHDesktop.SingleInstance")).is_ok() {
                // ERROR_ALREADY_EXISTS even on success means another instance holds it.
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    std::process::exit(0);
                }
                // Intentionally leak the handle: process exit releases the mutex.
            }
        }
    }

    let paths = paths::AppPaths::resolve();
    let _ = paths.ensure();

    let job = Arc::new(Job::create().expect("failed to create job object"));
    let state = Arc::new(ProcessState::new());

    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Start hidden to avoid a white flash while WebView2 initializes; the
    // window is revealed after the first presented frame (see below).
    let window = WindowBuilder::new()
        .with_visible(false)
        // Same product name the dsh web UI shows; avoids an obvious title
        // change when the webview loads.
        .with_title("DeepSeek Harness")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(720.0, 520.0))
        .with_window_icon(load_icon())
        .build(&event_loop)
        .expect("failed to build window");

    // Paint the native chrome in the content's colors BEFORE anything is
    // shown: at double-click launch the frame and the page must already match.
    #[cfg(windows)]
    let dark = system_prefers_dark();
    #[cfg(windows)]
    apply_window_chrome(&window, dark);
    // dsh's bg-base token: #f9fafb light / #151517 dark.
    #[cfg(windows)]
    let webview_bg: wry::RGBA = if dark { (21, 21, 23, 255) } else { (249, 250, 251, 255) };
    #[cfg(not(windows))]
    let webview_bg: wry::RGBA = (249, 250, 251, 255);

    let webview = WebViewBuilder::new()
        .with_html(init_ui::INIT_HTML)
        // Matches the init screen AND the dsh UI background, so the gap
        // between navigations never flashes a foreign color.
        .with_background_color(webview_bg)
        // Reveal the window only after the first frame is presented (double
        // rAF after DOMContentLoaded), so the user never sees a blank window.
        // The theme bridge observes dsh's own data-ds-dark-theme attribute and
        // reports every change (initial + in-app theme switches) so the native
        // chrome follows the page — the script re-runs on every navigation.
        .with_initialization_script(
            r#"
window.addEventListener('DOMContentLoaded', () => requestAnimationFrame(() => requestAnimationFrame(() => { try { window.ipc.postMessage('__visible'); } catch (e) {} })));
(function () {
  var reported;
  var start = function () {
    if (document.body && !reported) {
      reported = true;
      var report = function () {
        try { window.ipc.postMessage(document.body.hasAttribute('data-ds-dark-theme') ? '__theme:dark' : '__theme:light'); } catch (e) {}
      };
      new MutationObserver(report).observe(document.body, { attributes: true, attributeFilter: ['data-ds-dark-theme'] });
      report();
    }
  };
  document.addEventListener('DOMContentLoaded', start);
  setTimeout(start, 0);
})();
"#,
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
                // the native chrome follows the page.
                if body.contains("__theme:dark") {
                    #[cfg(windows)]
                    apply_window_chrome(&window, true);
                } else if body.contains("__theme:light") {
                    #[cfg(windows)]
                    apply_window_chrome(&window, false);
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
