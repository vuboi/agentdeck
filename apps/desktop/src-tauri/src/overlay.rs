//! A transparent webview stacked above the browser pages, for menus that open over a page.
//!
//! A page is a native layer: HTML of the app window can never be drawn above it. So a menu that overlaps the page
//! is mirrored here (its markup and the app's styles), placed exactly where it is, and clicks come back to the
//! real menu. The page stays live underneath; nothing is hidden or captured.

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl};

const LABEL: &str = "menu-overlay";

fn overlay(app: &AppHandle) -> Option<Webview> {
    app.get_webview(LABEL)
}

/// Creates the overlay, hidden, so showing it later is instant.
pub fn create(app: &AppHandle) -> Result<(), String> {
    if overlay(app).is_some() {
        return Ok(());
    }
    let window = app.get_window("main").ok_or("no main window")?;
    // static/overlay.html, served with the app.
    let builder = WebviewBuilder::new(LABEL, WebviewUrl::App("overlay.html".into()))
        .transparent(true)
        .focused(false);
    let view = window
        .add_child(builder, LogicalPosition::new(0.0, 0.0), LogicalSize::new(1.0, 1.0))
        .map_err(|error| error.to_string())?;
    view.hide().map_err(|error| error.to_string())
}

/// Runs a script in the overlay (styles, the menu markup).
#[tauri::command]
pub fn overlay_eval(app: AppHandle, script: String) -> Result<(), String> {
    overlay(&app)
        .ok_or("no overlay")?
        .eval(script)
        .map_err(|error| error.to_string())
}

/// Puts the overlay at a rectangle of the window, above every page.
#[tauri::command]
pub fn overlay_show(app: AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let view = overlay(&app).ok_or("no overlay")?;
    view.set_position(LogicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    view.set_size(LogicalSize::new(w.max(1.0), h.max(1.0)))
        .map_err(|error| error.to_string())?;
    view.show().map_err(|error| error.to_string())?;
    to_front(&view);
    Ok(())
}

#[tauri::command]
pub fn overlay_hide(app: AppHandle) -> Result<(), String> {
    match overlay(&app) {
        Some(view) => view.hide().map_err(|error| error.to_string()),
        None => Ok(()),
    }
}

/// Pages opened after the overlay are stacked above it; raise it again.
#[cfg(windows)]
fn to_front(view: &Webview) {
    let _ = view.with_webview(|platform| {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        // SAFETY: plain Win32 calls on the window that hosts this webview, on its own thread.
        unsafe {
            let mut parent = windows::Win32::Foundation::HWND::default();
            if platform.controller().ParentWindow(&mut parent).is_ok() {
                let _ = SetWindowPos(parent, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    });
}

#[cfg(not(windows))]
fn to_front(_view: &Webview) {}
