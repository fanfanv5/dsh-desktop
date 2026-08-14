//! Shared event/command types between the UI thread and the background controller.

use tao::event_loop::EventLoopProxy;

/// Commands the main (UI) thread sends to the background controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Re-run environment detection + install/upgrade + spawn.
    Retry,
    /// Install the latest version now (then restart the child).
    Upgrade,
    /// Skip the update and start the currently-installed version.
    SkipUpdate,
}

/// Events the background controller sends to the main (UI) thread.
pub enum UiEvent {
    /// Update the built-in initialization screen.
    Status { title: String, detail: String, actions: Vec<String> },
    /// The dsh web server is up; navigate the webview to this URL.
    Ready { url: String },
    /// Raw message received from the webview IPC bridge.
    Ipc(String),
}

pub type Proxy = EventLoopProxy<UiEvent>;
