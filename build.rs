//! Embeds the app icon into the Windows executable at build time.

fn main() {
    // Only meaningful for Windows builds; on other targets there is no PE to
    // decorate (and tauri-winres is only a dependency there).
    #[cfg(target_os = "windows")]
    {
        let mut res = tauri_winres::WindowsResource::new();
        res.set_icon("assets/dsh-desktop.ico");
        if let Err(e) = res.compile() {
            panic!("failed to embed Windows resources: {}", e);
        }
    }
}
