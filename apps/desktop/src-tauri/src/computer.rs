//! Approval-gated computer-use tools shared by the AgentDeck MCP bridge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT: AtomicU64 = AtomicU64::new(1);
static PENDING: LazyLock<Mutex<HashMap<String, oneshot::Sender<bool>>>> =
    LazyLock::new(Default::default);

#[derive(Serialize, Clone)]
struct ApprovalRequest {
    id: String,
    chat: String,
    tool: String,
    summary: String,
}

#[tauri::command]
pub fn computer_use_set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}

#[tauri::command]
pub fn computer_use_approval(id: String, allow: bool) -> Result<(), String> {
    let sender = PENDING
        .lock()
        .map_err(|_| "approval queue is unavailable".to_string())?
        .remove(&id)
        .ok_or_else(|| "approval request has expired".to_string())?;
    let _ = sender.send(allow);
    Ok(())
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub async fn approve(
    app: &AppHandle,
    chat: &str,
    tool: &str,
    summary: String,
) -> Result<(), String> {
    if !enabled() {
        return Err("Computer use is disabled in AgentDeck Settings.".into());
    }
    let id = format!("computer-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (sender, receiver) = oneshot::channel();
    PENDING
        .lock()
        .map_err(|_| "approval queue is unavailable".to_string())?
        .insert(id.clone(), sender);
    if app
        .emit(
            "computer-use-approval",
            ApprovalRequest {
                id: id.clone(),
                chat: chat.to_string(),
                tool: tool.to_string(),
                summary,
            },
        )
        .is_err()
    {
        if let Ok(mut pending) = PENDING.lock() {
            pending.remove(&id);
        }
        return Err("Could not show the approval request in AgentDeck.".into());
    }
    match tokio::time::timeout(std::time::Duration::from_secs(180), receiver).await {
        Ok(Ok(true)) if enabled() => Ok(()),
        Ok(Ok(true)) => Err("Computer use was disabled while waiting for approval.".into()),
        Ok(Ok(false)) => Err("The user denied this computer-use operation.".into()),
        Ok(Err(_)) => Err("The approval request was closed.".into()),
        Err(_) => {
            if let Ok(mut pending) = PENDING.lock() {
                pending.remove(&id);
            }
            Err("The approval request expired after 3 minutes.".into())
        }
    }
}

#[cfg(windows)]
pub fn screen_size() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

#[cfg(not(windows))]
pub fn screen_size() -> (i32, i32) {
    (0, 0)
}

#[cfg(windows)]
pub fn screenshot() -> Result<Vec<u8>, String> {
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if width <= 0 || height <= 0 || width as usize * height as usize > 80_000_000 {
        return Err("The display size is unavailable or too large to capture.".into());
    }
    unsafe {
        let screen = GetDC(None);
        if screen.0.is_null() {
            return Err("Could not access the display.".into());
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if memory.0.is_null() || bitmap.0.is_null() {
            let _ = ReleaseDC(None, screen);
            return Err("Could not allocate a screen capture.".into());
        }
        let old = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, width, height, Some(screen), 0, 0, SRCCOPY).is_ok();
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let lines = if copied {
            GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        let _ = SelectObject(memory, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        let _ = ReleaseDC(None, screen);
        if lines == 0 {
            return Err("The screen capture failed.".into());
        }
        let mut rgba = Vec::with_capacity(pixels.len());
        for bgra in pixels.chunks_exact(4) {
            rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
        }
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|e| e.to_string())?
            .write_image_data(&rgba)
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    }
}

#[cfg(not(windows))]
pub fn screenshot() -> Result<Vec<u8>, String> {
    Err("Computer use is currently supported on Windows only.".into())
}

#[cfg(windows)]
pub fn click(x: i32, y: i32) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
    unsafe {
        SetCursorPos(x, y).map_err(|e| e.to_string())?;
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn click(_: i32, _: i32) -> Result<(), String> {
    Err("Computer use is currently supported on Windows only.".into())
}

#[cfg(windows)]
pub fn type_text(text: &str) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };
    let mut events = Vec::with_capacity(text.encode_utf16().count() * 2);
    for unit in text.encode_utf16() {
        events.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: Default::default(),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        events.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: Default::default(),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }
    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
    if sent != events.len() as u32 {
        return Err("Windows did not accept all keyboard input.".into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn type_text(_: &str) -> Result<(), String> {
    Err("Computer use is currently supported on Windows only.".into())
}

#[cfg(windows)]
pub fn press_key(key: &str) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };
    let key = match key.to_ascii_lowercase().as_str() {
        "enter" => VK_RETURN,
        "tab" => VK_TAB,
        "escape" | "esc" => VK_ESCAPE,
        "backspace" => VK_BACK,
        "delete" => VK_DELETE,
        "space" => VK_SPACE,
        "up" | "arrowup" => VK_UP,
        "down" | "arrowdown" => VK_DOWN,
        "left" | "arrowleft" => VK_LEFT,
        "right" | "arrowright" => VK_RIGHT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pageup" => VK_PRIOR,
        "pagedown" => VK_NEXT,
        "ctrl" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        "f1" => VK_F1,
        "f2" => VK_F2,
        "f3" => VK_F3,
        "f4" => VK_F4,
        "f5" => VK_F5,
        "f6" => VK_F6,
        "f7" => VK_F7,
        "f8" => VK_F8,
        "f9" => VK_F9,
        "f10" => VK_F10,
        "f11" => VK_F11,
        "f12" => VK_F12,
        _ => return Err(
            "Unsupported key. Use Enter, Tab, Escape, arrows, editing keys, modifiers or F1–F12."
                .into(),
        ),
    };
    let input = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key.0),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let events = [input(Default::default()), input(KEYEVENTF_KEYUP)];
    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
    if sent != 2 {
        return Err("Windows did not accept the key press.".into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn press_key(_: &str) -> Result<(), String> {
    Err("Computer use is currently supported on Windows only.".into())
}

#[cfg(windows)]
pub fn scroll(delta: i32) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{mouse_event, MOUSEEVENTF_WHEEL};
    unsafe {
        mouse_event(MOUSEEVENTF_WHEEL, 0, 0, delta, 0);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn scroll(_: i32) -> Result<(), String> {
    Err("Computer use is currently supported on Windows only.".into())
}
