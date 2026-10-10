use crate::config::{atomic_write, Theme, WindowState};
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

pub struct DesktopShell {
    app: AppHandle,
    state: Mutex<WindowState>,
    directory: std::path::PathBuf,
}

impl DesktopShell {
    pub fn new(app: AppHandle, directory: std::path::PathBuf) -> Result<Self, String> {
        let state = std::fs::read(directory.join("state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let shell = Self {
            app,
            state: Mutex::new(state),
            directory,
        };
        let bubble = WebviewWindowBuilder::new(
            &shell.app,
            "bubble",
            WebviewUrl::App("index.html?surface=bubble".into()),
        )
        .title("bullet")
        .theme(Some(shell.theme().into()))
        .inner_size(52.0, 52.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?;
        WebviewWindowBuilder::new(
            &shell.app,
            "panel",
            WebviewUrl::App("index.html?surface=panel".into()),
        )
        .title("bullet")
        .theme(Some(shell.theme().into()))
        .inner_size(520.0, 116.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?;
        let saved = shell.state.lock().unwrap();
        let monitors = bubble
            .available_monitors()
            .map_err(|error| error.to_string())?;
        let valid_position = saved.bubble_x.zip(saved.bubble_y).filter(|(x, y)| {
            monitors.iter().any(|monitor| {
                let area = monitor.work_area();
                *x >= area.position.x
                    && *y >= area.position.y
                    && *x + 52 <= area.position.x + area.size.width as i32
                    && *y + 52 <= area.position.y + area.size.height as i32
            })
        });
        let position = if let Some((x, y)) = valid_position {
            PhysicalPosition::new(x, y)
        } else if let Some(monitor) = monitors.first() {
            let area = monitor.work_area();
            PhysicalPosition::new(
                area.position.x + area.size.width as i32 - 96,
                area.position.y + area.size.height as i32 - 144,
            )
        } else {
            PhysicalPosition::new(800, 600)
        };
        drop(saved);
        bubble
            .set_position(position)
            .map_err(|error| error.to_string())?;
        bubble.show().map_err(|error| error.to_string())?;
        Ok(shell)
    }
    pub fn app_handle(&self) -> &AppHandle {
        &self.app
    }
    pub fn toggle(&self) -> Result<(), String> {
        let panel = self.app.get_webview_window("panel").ok_or("面板不存在")?;
        if panel.is_visible().unwrap_or(false) {
            self.collapse()
        } else {
            self.show()
        }
    }
    pub fn show(&self) -> Result<(), String> {
        let panel = self.app.get_webview_window("panel").ok_or("面板不存在")?;
        self.resize(116.0)?;
        panel.show().map_err(|error| error.to_string())?;
        panel.set_focus().map_err(|error| error.to_string())?;
        let _ = panel.emit("panel-opened", ());
        Ok(())
    }
    pub fn collapse(&self) -> Result<(), String> {
        self.app
            .get_webview_window("panel")
            .ok_or("面板不存在")?
            .hide()
            .map_err(|error| error.to_string())?;
        let _ = self.app.emit_to("panel", "panel-collapsed", ());
        Ok(())
    }
    pub fn resize(&self, desired_height: f64) -> Result<f64, String> {
        let bubble = self.app.get_webview_window("bubble").ok_or("气泡不存在")?;
        let panel = self.app.get_webview_window("panel").ok_or("面板不存在")?;
        let position = bubble.outer_position().map_err(|error| error.to_string())?;
        let monitor = bubble
            .current_monitor()
            .map_err(|error| error.to_string())?
            .ok_or("无法获取显示器")?;
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let gap = (12.0 * scale) as i32;
        let width =
            (520.0 * scale).min((area.size.width as f64 - 2.0 * gap as f64).max(160.0)) as u32;
        let height = (desired_height.clamp(100.0, 720.0) * scale)
            .min((area.size.height as f64 - 2.0 * gap as f64).max(100.0))
            as u32;
        let bubble_size = bubble.outer_size().map_err(|error| error.to_string())?;
        let right = area.position.x + area.size.width as i32 - gap;
        let bottom = area.position.y + area.size.height as i32 - gap;
        let left_candidate = position.x - width as i32 - gap;
        let x = if left_candidate >= area.position.x + gap {
            left_candidate
        } else {
            position.x + bubble_size.width as i32 + gap
        };
        let x = x.clamp(
            area.position.x + gap,
            (right - width as i32).max(area.position.x + gap),
        );
        let y = position.y.clamp(
            area.position.y + gap,
            (bottom - height as i32).max(area.position.y + gap),
        );
        panel
            .set_size(PhysicalSize::new(width, height))
            .map_err(|error| error.to_string())?;
        panel
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
        Ok(height as f64 / scale)
    }
    pub fn moved(&self, position: PhysicalPosition<i32>) {
        let mut state = self.state.lock().unwrap();
        state.bubble_x = Some(position.x);
        state.bubble_y = Some(position.y);
        if let Ok(bytes) = serde_json::to_vec(&*state) {
            let _ = atomic_write(&self.directory.join("state.json"), &bytes);
        }
        drop(state);
        if let Some(panel) = self.app.get_webview_window("panel") {
            if panel.is_visible().unwrap_or(false) {
                let height = panel
                    .inner_size()
                    .ok()
                    .and_then(|size| {
                        panel
                            .scale_factor()
                            .ok()
                            .map(|scale| size.height as f64 / scale)
                    })
                    .unwrap_or(116.0);
                let _ = self.resize(height);
            }
        }
    }
    pub fn theme(&self) -> Theme {
        self.state.lock().unwrap().theme
    }
    pub fn set_theme(&self, theme: Theme) -> Result<Theme, String> {
        let mut state = self.state.lock().unwrap();
        let previous = state.theme;
        state.theme = theme;
        let result = serde_json::to_vec(&*state)
            .map_err(|error| error.to_string())
            .and_then(|bytes| atomic_write(&self.directory.join("state.json"), &bytes));
        if let Err(error) = result {
            state.theme = previous;
            return Err(error);
        }
        drop(state);
        for window in self.app.webview_windows().values() {
            let _ = window.set_theme(Some(theme.into()));
        }
        // Persistence is authoritative even if a WebView misses the notification.
        let _ = self.app.emit("theme-changed", theme);
        Ok(theme)
    }
    pub fn last_action(&self) -> Option<String> {
        self.state.lock().unwrap().last_action.clone()
    }
    pub fn select_action(&self, id: &str) {
        let mut state = self.state.lock().unwrap();
        state.last_action = Some(id.into());
        if let Ok(bytes) = serde_json::to_vec(&*state) {
            let _ = atomic_write(&self.directory.join("state.json"), &bytes);
        }
    }
    pub fn manage(&self, route: &str) -> Result<(), String> {
        let route = if route == "tasks" {
            "tasks"
        } else {
            "settings"
        };
        let window = match self.app.get_webview_window("manage") {
            Some(window) => window,
            None => WebviewWindowBuilder::new(
                &self.app,
                "manage",
                WebviewUrl::App(format!("index.html?surface=manage#{route}").into()),
            )
            .title("bullet · 管理")
            .theme(Some(self.theme().into()))
            .inner_size(980.0, 740.0)
            .min_inner_size(640.0, 480.0)
            .build()
            .map_err(|error| error.to_string())?,
        };
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        let _ = window.emit("navigate", route);
        Ok(())
    }
    pub fn show_bubble(&self, visible: bool) {
        if let Some(bubble) = self.app.get_webview_window("bubble") {
            if visible {
                let _ = bubble.show();
            } else {
                let _ = bubble.hide();
            }
        }
        if !visible {
            let _ = self.collapse();
        }
    }
    pub fn is_foreground(&self) -> bool {
        self.app.get_webview_window("panel").is_some_and(|window| {
            window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
        })
    }
    pub fn open_directory(&self) -> Result<(), String> {
        #[cfg(windows)]
        let mut command = std::process::Command::new("explorer.exe");
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("open");
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut command = std::process::Command::new("xdg-open");
        command
            .arg(&self.directory)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}
