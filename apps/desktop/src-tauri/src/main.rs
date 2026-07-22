#[tauri::command]
fn health() -> &'static str {
    "ghrm desktop core is available"
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![health])
        .run(tauri::generate_context!())
        .expect("failed to run GitHub Release Manager desktop app");
}
