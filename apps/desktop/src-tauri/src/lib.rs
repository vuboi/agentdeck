/// Starts the AgentDeck desktop window.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run AgentDeck desktop application");
}
