//! Vérification manuelle d'un plugin d'ampli sur la voie guitare :
//! `cargo run --release -p chelou-vst3-host --example amp -- --plugin "<bundle.vst3>" --device "Focusrite USB ASIO" --input 2`
//! Options : `--host asio`, `--rate 48000`, `--buffer 128`, `--seconds 60`, `--editor` (Windows :
//! ouvre l'interface du plugin ; la fermer termine l'exemple). Le plugin est chargé
//! sur le thread principal (comme dans l'app) ; imprime chaque seconde l'état, les crêtes et les
//! coupures.

use std::path::Path;
use std::time::Duration;

use chelou_audio_core::plugins::{binary_path, plugins_of};
use chelou_audio_core::settings::DuplexConfig;
use chelou_vst3_host::{probe, Editor, Vst3Plugin};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let bundle = arg("--plugin").expect("--plugin <bundle.vst3> requis");
    let bundle = Path::new(&bundle);
    let device = arg("--device").expect("--device <carte> requis");
    let config = DuplexConfig {
        host: arg("--host").unwrap_or_else(|| "asio".into()),
        output_device: device.clone(),
        input_device: device,
        input_channel: arg("--input")
            .map_or(2, |v| v.parse::<u16>().unwrap())
            .saturating_sub(1),
        output_pair: 0,
        sample_rate: arg("--rate").map_or(48000, |v| v.parse().unwrap()),
        buffer: arg("--buffer").map_or(128, |v| v.parse().unwrap()),
    };
    let seconds: u64 = arg("--seconds").map_or(60, |v| v.parse().unwrap());

    println!("binaire : {:?}", binary_path(bundle));
    let classes = probe(bundle)?;
    let info = plugins_of(bundle, &classes)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("aucune classe audio listée"))?;
    println!("plugin : {} — {} ({})", info.name, info.vendor, info.id);

    let engine = chelou_audio_engine::start(Some(config))?;
    println!("sortie {} Hz — {:?}", engine.sample_rate(), engine.state());
    let mut plugin = Vst3Plugin::load(bundle, &info.id, &info.name, engine.sample_rate())?;
    println!("latence plugin : {} échantillons", plugin.latency_samples());
    if engine
        .install_fx(plugin.take_rt(), engine.sample_rate())
        .is_err()
    {
        anyhow::bail!("la sortie a changé de fréquence pendant le chargement");
    }
    engine.reset_xruns();

    let editor = if args.iter().any(|a| a == "--editor") {
        #[cfg(windows)]
        // SAFETY: réglage DPI du processus, avant toute fenêtre (Tauri le fait pour l'app).
        unsafe {
            use windows::Win32::UI::HiDpi::*;
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        Some(Editor::open(
            &plugin,
            &format!("{} — Chelou Track", info.name),
            true,
        )?)
    } else {
        None
    };

    for s in 1..=seconds {
        if let Some(editor) = &editor {
            // Pompe les messages de la fenêtre pendant une seconde ; fermée → fin.
            for _ in 0..100 {
                #[cfg(windows)]
                chelou_vst3_host::editor::pump_messages();
                std::thread::sleep(Duration::from_millis(10));
            }
            if !editor.is_open() {
                println!("interface fermée");
                break;
            }
        } else {
            std::thread::sleep(Duration::from_secs(1));
        }
        let levels = engine.guitar().levels();
        println!(
            "[{s:>3}s] {:?}  entrée {:.3}  sortie {:.3}  coupures {}",
            engine.state(),
            levels.input,
            levels.output,
            engine.xruns()
        );
    }
    if let Some(editor) = editor {
        editor.close();
    }
    // Ordre imposé : fermer l'interface, retirer la partie temps réel de la voie, puis libérer le plugin.
    drop(engine.take_fx());
    drop(plugin);
    println!("plugin libéré proprement");
    Ok(())
}
