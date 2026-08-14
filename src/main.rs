#![windows_subsystem = "windows"]

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
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use events::{Command, UiEvent};
use process::{Job, ProcessState};

fn main() {
    let paths = paths::AppPaths::resolve();
    let _ = paths.ensure();

    let job = Arc::new(Job::create().expect("failed to create job object"));
    let state = Arc::new(ProcessState::new());

    let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("DSH Desktop")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 820.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(720.0, 520.0))
        .build(&event_loop)
        .expect("failed to build window");

    let webview = WebViewBuilder::new()
        .with_html(init_ui::INIT_HTML)
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

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UiEvent::Status { title, detail, actions }) => {
                send_status(&webview, &title, &detail, &actions);
            }
            Event::UserEvent(UiEvent::Ready { url }) => {
                let _ = webview.load_url(&url);
            }
            Event::UserEvent(UiEvent::Ipc(body)) => {
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
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                cleanup(&job, &state);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn cleanup(job: &Job, state: &ProcessState) {
    if state.assigned.load(Ordering::Relaxed) {
        job.kill_all();
    } else if let Some(pid) = *state.pid.lock().unwrap() {
        // Fallback for when the child could not be placed in the job object.
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
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
