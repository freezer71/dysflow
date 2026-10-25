//! Calcule `p-fenrir60` à partir de `ff_siwis` et `am_fenrir`.
//!
//!   cargo run --example kokoro_blend -- <dossier contenant voices/> <sortie.safetensors>

use dysflow::tts::kokoro::voices::VoicePack;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    anyhow::ensure!(args.len() == 3, "usage : kokoro_blend <dossier> <sortie>");
    let dir = std::path::Path::new(&args[1]).join("voices");
    let read = |name: &str| -> anyhow::Result<VoicePack> {
        VoicePack::from_safetensors(&std::fs::read(dir.join(format!("{name}.safetensors")))?)
    };
    let voice = VoicePack::blend_recipe(&read("ff_siwis")?, &read("am_fenrir")?);
    std::fs::write(&args[2], voice.to_safetensors())?;
    println!("écrit : {}", args[2]);
    Ok(())
}
