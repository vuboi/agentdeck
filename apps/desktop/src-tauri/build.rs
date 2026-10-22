fn main() {
    // The Windows taskbar icon is embedded in the executable. Make Cargo rebuild when it changes.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
