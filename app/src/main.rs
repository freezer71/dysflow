// Sous Windows, l'app de bureau s'ouvre sans fenêtre de console ; ses
// messages d'échec au démarrage passent par une boîte du système
// (`startup::system_alert`).
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    dysflow_app::run(std::env::args_os().nth(1).map(std::path::PathBuf::from));
}
