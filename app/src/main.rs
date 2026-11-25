// Sous Windows, l'app de bureau livrée (release) s'ouvre sans fenêtre de
// console ; ses messages d'échec au démarrage passent par une boîte du
// système (`startup::system_alert`). En debug, la console reste : les
// journaux et les paniques s'y lisent.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    dysflow_app::run(std::env::args_os().nth(1).map(std::path::PathBuf::from));
}
