#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_log::{Target, TargetKind};

mod commands;
mod crypto;
mod db;
mod state;

use commands::*;
use state::AppState;

pub fn run() {
    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir {
                        file_name: Some("argus.log".to_string()),
                    }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        // .plugin(tauri_plugin_sql::Builder::default().build())  // Disabled due to Send issues
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            start_bot,
            stop_bot,
            get_bot_status,
            list_guilds,
            list_channels,
            list_members,
            list_roles,
            send_message,
            send_dm,
            get_member_info,
            add_role,
            remove_role,
            timeout_member,
            get_config,
            save_config,
            get_server_map,
            refresh_server_map,
            get_active_guild,
            set_active_guild,
            get_config_full,
            save_config_full,
            list_bots,
            add_bot,
            update_bot,
            delete_bot,
            set_active_bot,
            list_templates,
            save_template,
            delete_template,
            apply_template,
            get_vigilance_config,
            save_vigilance_config,
            list_cli_agents,
            save_cli_agent,
            search_messages,
            cleanup_context,
            export_all_data,
            import_data,
            uninstall_wipe,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Run migrations synchronously
            let rt = tokio::runtime::Runtime::new().unwrap();
            let pool = rt.block_on(db::get_pool(&handle)).unwrap();
            rt.block_on(sqlx::migrate!("./migrations").run(&pool))
                .unwrap();

            // Note: Config loading is now done lazily when needed
            let _app_state = app.state::<AppState>();

            let show_item =
                MenuItem::with_id(&handle, "show", "Mostrar ARGUS", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(&handle, "quit", "Salir", true, None::<&str>)?;

            let menu = Menu::with_items(&handle, &[&show_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(handle.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } => {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(&handle)?;

            handle.on_menu_event(move |app, event| match event.id.as_ref() {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
