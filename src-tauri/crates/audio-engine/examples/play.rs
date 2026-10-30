//! Vérification manuelle du moteur : `cargo run -p chelou-audio-engine --example play -- <fichier>`.
//! Joue 5 s du fichier sur la sortie par défaut en imprimant le statut toutes les 500 ms.

use std::sync::Arc;
use std::time::Duration;

use chelou_audio_core::decode::{decode_stereo, resample};

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("usage : play <fichier audio>");
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_owned);
    let engine = chelou_audio_engine::start(None)?;
    let pcm = resample(
        &decode_stereo(std::fs::read(&path)?, ext.as_deref())?,
        engine.sample_rate(),
    )?;
    println!(
        "sortie {} Hz, track {:.0} ms",
        engine.sample_rate(),
        pcm.duration_ms()
    );
    engine.backing().load(Arc::new(pcm));
    engine.backing().play();
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(500));
        println!("{:?} {:?}", engine.backing().status(), engine.state());
    }
    Ok(())
}
