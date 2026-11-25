//! Le crochet de panique du démarrage, seul dans son processus : il est
//! global, et les autres essais ne doivent pas passer par lui. Aucune
//! fenêtre, aucun GPU : les paniques sont provoquées par l'essai et
//! rattrapées (`catch_unwind`, `join` ; profil de test : `panic = "unwind"`).

use std::panic::catch_unwind;
use std::sync::{Arc, Mutex};

use dysflow_app::startup::{install_startup_hook, no_gpu_message, remove_startup_hook};

#[test]
fn a_startup_panic_writes_the_no_gpu_message_then_the_hook_goes_away() {
    let written = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&written);
    // Posé sur ce fil, comme `run` sur le fil principal avant GPUI.
    install_startup_hook(move |message| sink.lock().unwrap().push_str(message));

    // Comme `WindowsPlatform::connect_initially` sans DirectX.
    let crashed = catch_unwind(|| -> () {
        panic!("failed to initialize DirectX: no adapter");
    });
    assert!(crashed.is_err());
    // Une panique au message formaté (`String`), comme celle de Linux.
    let crashed = catch_unwind(|| -> () {
        let display = ":0";
        panic!("failed to connect to X server {display}");
    });
    assert!(crashed.is_err());
    // Le fil du lecteur (un autre fil) qui panique n'en parle pas.
    let other = std::thread::spawn(|| -> () { panic!("fil du lecteur") }).join();
    assert!(other.is_err());
    assert_eq!(
        written.lock().unwrap().as_str(),
        format!(
            "{}{}",
            no_gpu_message("failed to initialize DirectX: no adapter"),
            no_gpu_message("failed to connect to X server :0")
        )
    );

    // Retiré (début du rappel de `run`) : les paniques suivantes gardent
    // leur comportement normal, sans message de GPU.
    remove_startup_hook();
    let later = catch_unwind(|| -> () { panic!("plus tard") });
    assert!(later.is_err());
    assert!(!written.lock().unwrap().contains("plus tard"));
}
