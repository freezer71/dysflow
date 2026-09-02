//! Le presse-papiers comme source : ce qu'il faut, et ce qu'il ne faut pas.

use std::process::Command;

#[test]
fn pasting_text_asks_for_no_recognition() {
    // Le texte du presse-papiers n'a rien à reconnaître. L'exiger quand même
    // fermait la porte à tout binaire construit par défaut — le plus courant.
    let out = Command::new(env!("CARGO_BIN_EXE_sillon"))
        .args(["--clip", "--to-markdown"])
        .output()
        .expect("le binaire doit se lancer");

    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("option `ocr`"),
        "coller du texte ne demande pas de reconnaissance : {err}"
    );
}
