//! Alterix app setup: shared state, plugins, commands, single-instance handling,
//! command-line paths and the main window.

pub mod applog;
mod check;
mod commands;
pub mod engine;
pub mod feedback;
pub mod listing;
pub mod meta;
pub mod ops;
pub mod script;
pub mod shell;
pub mod store;

use commands::AppState;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// Command-line arguments that name files, folders or a preset (not OS flags).
fn path_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with("-psn_") && !a.starts_with("--"))
        .collect()
}

pub fn run() {
    let context = tauri::generate_context!();
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = check::from_args(&args, &context) {
        std::process::exit(code);
    }
    let store = store::Store::new();
    let status = store.status();
    applog::start(&store.dir(), status.writable);
    applog::info("app", "Starting");
    let allow_many = store
        .setting_bool("allowMultipleInstances")
        .unwrap_or(false);
    let scripts = script::ScriptState::default();
    *scripts.scripts_dir.lock().unwrap() = Some(store.dir().join("scripts"));
    let state = AppState {
        store,
        cache: engine::InfoCache::default(),
        scripts,
        launch_args: Mutex::new(path_args(std::env::args())),
    };

    let mut builder = tauri::Builder::default();
    if !allow_many {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let paths = path_args(args);
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            if !paths.is_empty() {
                let _ = app.emit("open-paths", paths);
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::data_status,
            commands::use_data_folder,
            commands::load_settings,
            commands::save_settings,
            commands::default_preset,
            commands::save_default_preset,
            commands::launch_args,
            commands::scan,
            commands::stat_paths,
            commands::columns,
            commands::properties,
            commands::image_preview,
            commands::preview,
            commands::rename,
            commands::manual_rename,
            commands::history,
            commands::undo,
            commands::clear_history,
            commands::save_undo_script,
            commands::log_file,
            commands::read_text_file,
            commands::write_text_file,
            commands::find_autoload,
            commands::parse_pairs,
            commands::shell_status,
            commands::shell_install,
            commands::shell_uninstall,
            commands::refresh_paths,
            commands::app_info,
            commands::log_line,
            commands::log_path,
        ])
        .setup(|app| {
            // Incognito keeps the web view from writing caches or storage to
            // AppData / ~/Library. App data lives in Alterix-data/ beside the app.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Alterix")
                .inner_size(1280.0, 800.0)
                .min_inner_size(960.0, 600.0)
                .incognito(true)
                .build()?;
            #[cfg(target_os = "macos")]
            add_help_menu(app)?;
            applog::info("app", "Window opened");
            Ok(())
        })
        .build(context)
        .expect("error while starting Alterix")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                applog::end();
            }
        });
}

/// macOS has a menu bar: add Help > Report a Problem… and Help > View Logs.
#[cfg(target_os = "macos")]
fn add_help_menu(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, Submenu};
    use tauri_plugin_opener::OpenerExt;
    let handle = app.handle();
    let menu = Menu::default(handle)?;
    let report = MenuItem::with_id(handle, "report", "Report a Problem…", true, None::<&str>)?;
    let logs = MenuItem::with_id(handle, "logs", "View Logs", true, None::<&str>)?;
    let help = match menu.items()?.into_iter().find_map(|i| {
        i.as_submenu()
            .filter(|s| s.text().map(|t| t == "Help").unwrap_or(false))
            .cloned()
    }) {
        Some(h) => h,
        None => {
            let h = Submenu::new(handle, "Help", true)?;
            menu.append(&h)?;
            h
        }
    };
    help.append(&report)?;
    help.append(&logs)?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        "report" => {
            let url = feedback::report_url(&applog::version(), &applog::os_text());
            applog::info("feedback", "Report a Problem opened in the browser");
            let _ = app.opener().open_url(url, None::<&str>);
        }
        "logs" => {
            if let Some(p) = applog::path() {
                let _ = app.opener().reveal_item_in_dir(p);
            }
        }
        _ => {}
    });
    Ok(())
}
