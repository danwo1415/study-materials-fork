fn main() {
    tauri_plugin::Builder::new(&["play"])
        .android_path("android")
        .build();
}
