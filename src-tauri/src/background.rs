//! Keeping work going after the window is closed.
//!
//! Closing the window (or Cmd+Q) while a conversion or copy is running only
//! hides the window. When the work finishes she gets a notification, and if
//! the window is still hidden the app quits.

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Manager, RunEvent, Window, WindowEvent, Wry};
use tauri_plugin_notification::NotificationExt;

use crate::AppState;

fn busy(app: &AppHandle) -> bool {
    app.state::<AppState>().busy()
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

const QUIT_ID: &str = "quit";

/// The standard macOS menu, except Quit is our own item: the built-in one
/// terminates the app without giving us a chance to keep work running.
pub fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit Pixieflow", true, Some("CmdOrCtrl+Q"))?;
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                "Pixieflow",
                true,
                &[
                    &PredefinedMenuItem::about(app, None, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::show_all(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_id_and_items(
                app,
                WINDOW_SUBMENU_ID,
                "Window",
                true,
                &[
                    &PredefinedMenuItem::minimize(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::close_window(app, None)?,
                ],
            )?,
            &Submenu::with_id_and_items(app, HELP_SUBMENU_ID, "Help", true, &[])?,
        ],
    )
}

pub fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    if event.id() == QUIT_ID {
        if busy(app) {
            hide_main_window(app);
        } else {
            app.exit(0);
        }
    }
}

pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if busy(window.app_handle()) {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

pub fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        // Other ways of quitting that do reach us. `code` is only set when
        // we call `app.exit()` ourselves.
        RunEvent::ExitRequested { code: None, api, .. } if busy(app) => {
            api.prevent_exit();
            hide_main_window(app);
        }
        // Clicking the Dock icon while the window is hidden.
        RunEvent::Reopen { .. } => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        _ => {}
    }
}

/// Call once a job is no longer counted as busy. Notifies her unless she's
/// looking at the app, and quits if the window was closed in the meantime.
pub fn job_finished(app: &AppHandle, title: &str, body: &str) {
    let window = app.get_webview_window("main");
    let visible = window.as_ref().is_some_and(|w| w.is_visible().unwrap_or(false));
    let focused = window.as_ref().is_some_and(|w| w.is_focused().unwrap_or(false));

    if !(visible && focused) {
        let _ = app.notification().builder().title(title).body(body).show();
    }
    if !visible && !busy(app) {
        // The notification is posted asynchronously; quitting right away
        // would drop it. Re-check afterwards in case she reopened the window.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            let visible = app.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false));
            if !visible && !busy(&app) {
                app.exit(0);
            }
        });
    }
}
