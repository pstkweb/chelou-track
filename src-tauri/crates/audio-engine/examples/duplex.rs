//! Vérification manuelle du son direct :
//! `cargo run -p chelou-audio-engine --example duplex -- --host asio --device "Focusrite USB ASIO" --input 2`
//! Options : `--output <carte>` (WASAPI : nom de la sortie), `--rate 48000`, `--buffer 128`,
//! `--seconds 60`. Imprime chaque seconde l'état, les crêtes d'entrée/sortie et les coupures.

use std::time::Duration;

use chelou_audio_core::settings::DuplexConfig;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let device = arg("--device").expect("--device <carte> requis");
    let config = DuplexConfig {
        host: arg("--host").unwrap_or_else(|| "asio".into()),
        output_device: arg("--output").unwrap_or_else(|| device.clone()),
        input_device: device,
        input_channel: arg("--input").map_or(1, |v| v.parse::<u16>().unwrap()) - 1,
        output_pair: 0,
        sample_rate: arg("--rate").map_or(48000, |v| v.parse().unwrap()),
        buffer: arg("--buffer").map_or(128, |v| v.parse().unwrap()),
    };
    let seconds: u64 = arg("--seconds").map_or(60, |v| v.parse().unwrap());

    println!("inventaire :");
    for host in chelou_audio_engine::list_devices() {
        for d in host.devices {
            println!("  [{}] {d:?}", host.host);
        }
    }
    let engine = chelou_audio_engine::start(Some(config))?;
    println!("sortie {} Hz — {:?}", engine.sample_rate(), engine.state());
    for s in 1..=seconds {
        std::thread::sleep(Duration::from_secs(1));
        let levels = engine.guitar().levels();
        println!(
            "[{s:>3}s] {:?}  entrée {:.3}  sortie {:.3}  coupures {}",
            engine.state(),
            levels.input,
            levels.output,
            engine.xruns()
        );
    }
    Ok(())
}
