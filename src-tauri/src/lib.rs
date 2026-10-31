use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_log::{Target, TargetKind};

pub mod commands;
pub mod crypto;
pub mod db;
pub mod discord;
pub mod gateway;
pub mod i18n;
pub mod mapa;
pub mod mcp;
pub mod mensajes;
pub mod persistencia;
pub mod propuesta;
pub mod state;
pub mod sync;
#[cfg(test)]
mod tests_ajustes;
#[cfg(test)]
mod tests_canales;
#[cfg(test)]
mod tests_mensajes;
#[cfg(test)]
mod tests_moderacion;
#[cfg(test)]
mod tests_permisos_canal;
#[cfg(test)]
mod tests_roles;

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
            bot_permissions,
            create_channel,
            update_channel,
            delete_channel,
            reorder_channels,
            create_role,
            update_role,
            delete_role,
            reorder_roles,
            set_channel_permission,
            delete_channel_permission,
            get_guild_settings_discord,
            update_guild_discord,
            audit_log,
            channel_messages,
            delete_message,
            bulk_delete_messages,
            pin_message,
            unpin_message,
            create_invite,
            list_invites,
            delete_invite,
            list_webhooks,
            create_webhook,
            delete_webhook,
            moderation_context,
            kick_member,
            ban_member,
            unban_member,
            list_bans,
            set_nickname,
            guild_info,
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
            resync_map,
            get_active_guild,
            set_active_guild,
            get_config_full,
            save_config_full,
            list_bots,
            add_bot,
            update_bot,
            delete_bot,
            set_active_bot,
            invite_url,
            ruta_mcp,
            list_templates,
            save_template,
            delete_template,
            apply_template,
            get_vigilance_config,
            get_guild_settings,
            save_guild_settings,
            save_vigilance_config,
            list_cli_agents,
            save_cli_agent,
            search_messages,
            cleanup_context,
            export_all_data,
            import_data,
            uninstall_wipe,
            propuesta::validar_propuesta,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Abre la BD (y aplica migraciones) en el runtime de Tauri y la
            // registra ANTES de que nada la pida con `db::get_pool`.
            // `ARGUS_DB` tiene prioridad sobre la ruta por defecto.
            let ruta = db::ruta_bd(&handle)?;
            let pool = tauri::async_runtime::block_on(db::abrir_bd_por_ruta(&ruta))?;
            app.manage(pool);

            // Load the saved config; a failure is logged but must not abort startup
            let app_state = app.state::<AppState>();
            if let Err(e) = tauri::async_runtime::block_on(app_state.load_config_from_db(&handle)) {
                tracing::error!("no se pudo cargar la configuración guardada: {e}");
            }

            // Recupera el último mapa guardado y lo guarda cada 30 s si cambió
            match tauri::async_runtime::block_on(async {
                let pool = db::get_pool(&handle).await?;
                let guardados = persistencia::cargar_mapas(&pool).await?;
                Ok::<_, state::DynError>((pool, guardados))
            }) {
                Ok((pool, guardados)) => {
                    let ultimo = persistencia::instantanea(&guardados);
                    app_state.server_maps.lock().unwrap().extend(guardados);
                    let mapas = app_state.server_maps.clone();
                    tauri::async_runtime::spawn(async move {
                        persistencia::lanzar_guardado(
                            pool,
                            mapas,
                            ultimo,
                            persistencia::INTERVALO_GUARDADO,
                        );
                    });
                }
                Err(e) => tracing::error!("no se pudo cargar el mapa guardado: {e}"),
            }

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
