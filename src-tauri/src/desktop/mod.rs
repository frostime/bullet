mod shell;

use crate::{
    config::{
        model::{Config, NotificationMode},
        ConfigSnapshot, ConfigStore, ScriptSnapshot,
    },
    execution::{RunEvent, RunManager, RunStatus, RunSummary},
};
use shell::DesktopShell;
use std::sync::Arc;
use tauri::{
    ipc::Channel,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State,
};
use tauri_plugin_notification::NotificationExt;

struct AppState {
    config: Arc<ConfigStore>,
    runs: Arc<RunManager>,
    shell: DesktopShell,
}

#[tauri::command]
fn load_config(state: State<AppState>) -> ConfigSnapshot {
    state.config.load()
}
#[tauri::command]
fn save_config(
    state: State<AppState>,
    config: Config,
    revision: String,
) -> Result<ConfigSnapshot, String> {
    state.config.save(config, &revision)
}
#[tauri::command]
fn read_script(
    state: State<AppState>,
    id: String,
    extension: String,
) -> Result<ScriptSnapshot, String> {
    state.config.read_script(&id, &extension)
}
#[tauri::command]
fn save_script(
    state: State<AppState>,
    id: String,
    extension: String,
    source: String,
    revision: String,
) -> Result<ScriptSnapshot, String> {
    state
        .config
        .save_script(&id, &extension, &source, &revision)
}
#[tauri::command]
async fn start_run(
    state: State<'_, AppState>,
    action_id: String,
    text: String,
) -> Result<String, String> {
    state.shell.select_action(&action_id);
    state.runs.start(&action_id, text)
}
#[tauri::command]
fn attach_run(
    state: State<AppState>,
    run_id: String,
    channel: Channel<RunEvent>,
) -> Result<(), String> {
    state.runs.set_foreground(Some(run_id.clone()));
    state
        .runs
        .subscribe(&run_id, Arc::new(move |event| channel.send(event).is_ok()))
}
#[tauri::command]
fn cancel_run(state: State<AppState>, run_id: String) -> Result<(), String> {
    state.runs.cancel(&run_id)
}
#[tauri::command]
fn detach_run(state: State<AppState>, run_id: String) {
    state.runs.detach(&run_id);
}
#[tauri::command]
fn list_runs(state: State<AppState>) -> Vec<RunSummary> {
    state.runs.list()
}
#[tauri::command]
fn clear_history(state: State<AppState>) -> Result<(), String> {
    state.runs.clear_history()
}
#[tauri::command]
fn toggle_panel(state: State<AppState>) -> Result<(), String> {
    state.shell.toggle()
}
#[tauri::command]
fn collapse_panel(state: State<AppState>) -> Result<(), String> {
    state.shell.collapse()
}
#[tauri::command]
fn resize_panel(state: State<AppState>, height: f64) -> Result<f64, String> {
    state.shell.resize(height)
}
#[tauri::command]
async fn open_management(state: State<'_, AppState>, route: String) -> Result<(), String> {
    state.shell.manage(&route)
}
#[tauri::command]
fn open_config_directory(state: State<AppState>) -> Result<(), String> {
    state.shell.open_directory()
}
#[tauri::command]
fn selected_action(state: State<AppState>) -> Option<String> {
    state.shell.last_action()
}
#[tauri::command]
fn select_action(state: State<AppState>, id: String) {
    state.shell.select_action(&id);
}
#[tauri::command]
fn resolve_action_cwd(
    state: State<AppState>,
    cwd: Option<String>,
    default_cwd: String,
) -> Result<String, String> {
    crate::config::paths::resolve_cwd(&default_cwd, cwd.as_deref(), &state.config.home)
        .map(|path| path.to_string_lossy().into_owned())
}
#[tauri::command]
fn show_run(state: State<AppState>, run_id: String) -> Result<(), String> {
    state.runs.set_foreground(Some(run_id.clone()));
    state.shell.show()?;
    use tauri::Emitter;
    state
        .shell
        .app_handle()
        .emit_to("panel", "show-run", run_id)
        .map_err(|error| error.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(state) = app.try_state::<AppState>() {
                let _ = state.shell.show();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            read_script,
            save_script,
            start_run,
            attach_run,
            cancel_run,
            detach_run,
            list_runs,
            clear_history,
            toggle_panel,
            collapse_panel,
            resize_panel,
            open_management,
            open_config_directory,
            selected_action,
            select_action,
            resolve_action_cwd,
            show_run
        ])
        .setup(|app| {
            let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
            let config = Arc::new(ConfigStore::open(home.join(".config/bullet"), home)?);
            let notification_app = app.handle().clone();
            let runs = RunManager::new(
                config.clone(),
                Arc::new(move |run, mode| {
                    let Some(state) = notification_app.try_state::<AppState>() else {
                        return;
                    };
                    let foreground = state.runs.foreground().as_deref() == Some(&run.id)
                        && state.shell.is_foreground();
                    if foreground {
                        state.runs.mark_read(&run.id);
                    }
                    if run.status == RunStatus::Cancelled
                        || mode == NotificationMode::Off
                        || (mode == NotificationMode::Background && foreground)
                    {
                        return;
                    }
                    let body = if run.status == RunStatus::Succeeded {
                        "任务已完成，可在任务列表查看结果"
                    } else {
                        "任务失败，可在任务列表查看详情"
                    };
                    if let Err(error) = notification_app
                        .notification()
                        .builder()
                        .title(format!("bullet · {}", run.action_name))
                        .body(body)
                        .show()
                    {
                        state.runs.append(
                            &run.id,
                            true,
                            &format!("\n系统通知未发送：{error}"),
                            262_144,
                        );
                    }
                }),
            );
            let shell = DesktopShell::new(app.handle().clone(), config.root.clone())?;
            app.manage(AppState {
                config,
                runs,
                shell,
            });
            let show = MenuItem::with_id(app, "show", "打开输入", true, None::<&str>)?;
            let hide = MenuItem::with_id(app, "hide", "隐藏气泡", true, None::<&str>)?;
            let bubble = MenuItem::with_id(app, "bubble", "显示气泡", true, None::<&str>)?;
            let tasks = MenuItem::with_id(app, "tasks", "任务与最近结果", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "配置动作", true, None::<&str>)?;
            let directory =
                MenuItem::with_id(app, "directory", "打开配置目录", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 bullet", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&show, &hide, &bubble, &tasks, &settings, &directory, &quit],
            )?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon().cloned().unwrap_or_else(|| {
                        tauri::image::Image::new_owned(vec![20, 20, 20, 255], 1, 1)
                    }),
                )
                .tooltip("bullet")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let state = app.state::<AppState>();
                    match event.id.as_ref() {
                        "show" => {
                            let _ = state.shell.show();
                        }
                        "hide" => state.shell.show_bubble(false),
                        "bubble" => state.shell.show_bubble(true),
                        "tasks" | "settings" => {
                            let app = app.clone();
                            let route = event.id.as_ref().to_owned();
                            tauri::async_runtime::spawn(async move {
                                let _ = app.state::<AppState>().shell.manage(&route);
                            });
                        }
                        "directory" => {
                            let _ = state.shell.open_directory();
                        }
                        "quit" => {
                            state.runs.cancel_all();
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                                app.exit(0);
                            });
                        }
                        _ => (),
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        let _ = tray.app_handle().state::<AppState>().shell.show();
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let Some(state) = window.app_handle().try_state::<AppState>() else {
                return;
            };
            if window.label() == "bubble" {
                if let tauri::WindowEvent::Moved(position) = event {
                    state.shell.moved(*position);
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("bullet failed to start");
}
