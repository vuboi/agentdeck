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

/// A press on the mirror gave the overlay the keyboard: hand it back to the app, whose field (the search of a
/// mirrored dialog, say) still has the focus inside it, so typing goes on there.
#[tauri::command]
pub fn overlay_return_focus(app: AppHandle) -> Result<(), String> {
    app.get_webview("main")
        .ok_or("no main webview")?
        .set_focus()
        .map_err(|error| error.to_string())
}

/// Bumped on every show: a hide that waited for the closing animation is dropped when a menu opened since.
static SHOWN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Whether a menu is on screen in the overlay (for screenshots of the page).
static VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The overlay, when a menu is on screen in it.
pub fn visible_view(app: &AppHandle) -> Option<Webview> {
    if VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
        overlay(app)
    } else {
        None
    }
}

/// Draws the agent cursor above a mirrored menu. The page cursor cannot cross this native webview layer.
pub fn pointer(app: &AppHandle, x: f64, y: f64, kind: &str) {
    let Some(view) = visible_view(app) else {
        return;
    };
    let Ok(position) = view.position() else {
        return;
    };
    let local_x = x - f64::from(position.x);
    let local_y = y - f64::from(position.y);
    let script = format!(
        "window.__overlay && window.__overlay.pointer({local_x}, {local_y}, {kind:?})"
    );
    let _ = view.eval(&script);
}

/// Runs `script` (the menu markup) and puts the overlay at a rectangle of the window, above every page, in one
/// call: the menu appears without a round trip in between.
#[tauri::command]
pub fn overlay_show(app: AppHandle, script: Option<String>, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    SHOWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let view = overlay(&app).ok_or("no overlay")?;
    if let Some(script) = script {
        view.eval(script).map_err(|error| error.to_string())?;
    }
    view.set_position(LogicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    view.set_size(LogicalSize::new(w.max(1.0), h.max(1.0)))
        .map_err(|error| error.to_string())?;
    view.show().map_err(|error| error.to_string())?;
    to_front(&view);
    // Showing a webview can give it the keyboard: it stays with the app, whose field (the palette's search) has it.
    if let Some(main) = app.get_webview("main") {
        let _ = main.set_focus();
    }
    VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Plays the closing animation, then hides the overlay (unless a menu opened meanwhile).
#[tauri::command]
pub async fn overlay_hide(app: AppHandle) -> Result<(), String> {
    let Some(view) = overlay(&app) else {
        return Ok(());
    };
    let shown = SHOWN.load(std::sync::atomic::Ordering::Relaxed);
    VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = view.eval("window.__overlay && window.__overlay.close()");
    tokio::time::sleep(std::time::Duration::from_millis(130)).await;
    if SHOWN.load(std::sync::atomic::Ordering::Relaxed) == shown {
        let _ = view.eval("window.__overlay && window.__overlay.clear()");
        view.hide().map_err(|error| error.to_string())?;
    }
    Ok(())
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
