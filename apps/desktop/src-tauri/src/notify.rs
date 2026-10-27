//! System notifications on Windows with AgentDeck's logo. The notification plugin sends a toast without one (the
//! icon it is given is not used on Windows), so the toast here carries the app icon as its logo. macOS and Linux show
//! the app's own icon already: the frontend sends those through the plugin.

/// Sends a toast with the AgentDeck logo. `Ok(false)` on the systems where the plugin is used instead.
#[tauri::command]
pub async fn system_notify(app: tauri::AppHandle, title: String, body: String) -> Result<bool, String> {
    #[cfg(windows)]
    {
        use tauri_winrt_notification::{IconCrop, Toast};
        // An installed app has its own identity (name and icon in the toast's header). A dev build runs from
        // target/debug and has none registered: Windows then only shows a toast sent as PowerShell.
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let mut folders = exe.iter().rev().skip(1).map(|part| part.to_string_lossy().to_lowercase());
        let profile = folders.next().unwrap_or_default();
        let dev = folders.next().as_deref() == Some("target") && (profile == "debug" || profile == "release");
        let identifier = app.config().identifier.clone();
        let app_id = if dev { Toast::POWERSHELL_APP_ID } else { identifier.as_str() };
        let logo = std::env::temp_dir().join("agentdeck-notification-logo.png");
        if !logo.exists() {
            std::fs::write(&logo, include_bytes!("../icons/128x128.png")).map_err(|error| error.to_string())?;
        }
        Toast::new(app_id)
            .icon(&logo, IconCrop::Square, "AgentDeck")
            .title(&title)
            .text1(&body)
            .show()
            .map_err(|error| error.to_string())?;
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, title, body);
        Ok(false)
    }
}
