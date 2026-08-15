//! Borderless chrome: remove the native title bar while keeping native
//! resize, snap and maximize behavior.
//!
//! WS_CAPTION is stripped after window creation and the window proc is
//! subclassed to answer WM_NCCALCSIZE (client area covers the whole window)
//! and WM_NCHITTEST (invisible resize frame). The in-page header bar
//! (injected by the webview init script) drives moving/controls through IPC.

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, IsZoomed,
    SetWindowLongPtrW, SetWindowPos, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT,
    HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, GWL_STYLE, GWLP_WNDPROC, SM_CXPADDEDBORDER,
    SM_CXSIZEFRAME, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOZORDER, WM_NCCALCSIZE, WM_NCHITTEST,
    WS_CAPTION, NCCALCSIZE_PARAMS,
};

#[cfg(windows)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(windows)]
static ORIGINAL_PROC: AtomicUsize = AtomicUsize::new(0);

/// Total invisible resize frame thickness (size frame + DWM padding).
#[cfg(windows)]
fn frame_thickness() -> i32 {
    unsafe {
        GetSystemMetrics(SM_CXSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER)
    }
}

#[cfg(windows)]
fn original_proc() -> usize {
    // Written once from the main thread before any message can arrive
    // through the subclass; read-only afterwards.
    ORIGINAL_PROC.load(Ordering::Acquire)
}

#[cfg(windows)]
unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE if wparam.0 != 0 => {
            // Client area = whole window; when maximized keep a frame-sized
            // top/left/right inset so the content doesn't bleed past the
            // screen edges and the taskbar.
            let params = lparam.0 as *mut NCCALCSIZE_PARAMS;
            if IsZoomed(hwnd).as_bool() {
                let f = frame_thickness();
                (*params).rgrc[0].top += f;
                (*params).rgrc[0].left += f;
                (*params).rgrc[0].right -= f;
                (*params).rgrc[0].bottom -= f;
            }
            return LRESULT(0);
        }
        WM_NCHITTEST => {
            let prev = call_original(hwnd, msg, wparam, lparam);
            if prev.0 != HTCLIENT as isize || IsZoomed(hwnd).as_bool() {
                return prev;
            }
            // Claim an invisible resize border on the window edges.
            let x = (lparam.0 & 0xFFFF) as u16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i32;
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return prev;
            }
            let m = frame_thickness() + 2;
            let left = x - rect.left < m;
            let right = rect.right - x < m;
            let top = y - rect.top < m;
            let bottom = rect.bottom - y < m;
            let hit = if top && left {
                HTTOPLEFT
            } else if top && right {
                HTTOPRIGHT
            } else if bottom && left {
                HTBOTTOMLEFT
            } else if bottom && right {
                HTBOTTOMRIGHT
            } else if left {
                HTLEFT
            } else if right {
                HTRIGHT
            } else if top {
                HTTOP
            } else if bottom {
                HTBOTTOM
            } else {
                return prev;
            };
            return LRESULT(hit as isize);
        }
        _ => {}
    }
    call_original(hwnd, msg, wparam, lparam)
}

#[cfg(windows)]
unsafe fn call_original(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let proc: windows::Win32::UI::WindowsAndMessaging::WNDPROC =
        Some(std::mem::transmute(original_proc()));
    CallWindowProcW(proc, hwnd, msg, wparam, lparam)
}

/// Strip the caption and install the subclass. Call once after the window is
/// built, on the thread that owns the window.
#[cfg(windows)]
pub fn apply(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE;
    use windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        // Keep WS_THICKFRAME / SYSMENU / min-max boxes: resize, snap and
        // maximize stay native, only the caption goes away.
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_CAPTION.0 as isize));
        ORIGINAL_PROC.store(GetWindowLongPtrW(hwnd, GWLP_WNDPROC) as usize, Ordering::Release);
        SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            subclass_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT
                as isize,
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// Enter the native "move window" modal loop from an in-page header drag.
/// Gives the standard drag/snap experience, including double-click to
/// maximize, exactly like a real title bar.
#[cfg(windows)]
pub fn begin_drag(hwnd: HWND) {
    use windows::Win32::Foundation::WPARAM;
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_NCLBUTTONDOWN};
    unsafe {
        let _ = ReleaseCapture();
        let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, Some(WPARAM(HTCAPTION as usize)), None);
    }
}
