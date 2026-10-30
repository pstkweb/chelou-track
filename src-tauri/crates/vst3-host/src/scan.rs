// Scan des VST3 (spec §5.6) : `moduleinfo.json` lu sans rien charger ; sinon le bundle est chargé
// dans un processus enfant (le binaire de l'app relancé avec `--vst3-probe <bundle>`) — charger
// une DLL exécute son code, un plugin qui plante ou boucle ne doit pas toucher l'app.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chelou_audio_core::plugins::{
    dedupe, find_bundles, module_info_path, parse_module_info, plugins_of, ClassInfo, PluginInfo,
};

/// Argument qui fait imprimer au binaire de l'app les classes d'un bundle (JSON) puis sortir.
pub const PROBE_ARG: &str = "--vst3-probe";
/// Au-delà, le processus enfant est tué et le bundle ignoré.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Plugins de `folders` (dossiers standard d'abord) ; `exe` = binaire qui sait répondre à
/// `PROBE_ARG`. Un bundle illisible est ignoré (log sur stderr).
pub fn scan(folders: &[PathBuf], exe: &Path) -> Vec<PluginInfo> {
    let mut plugins = Vec::new();
    for bundle in find_bundles(folders) {
        let classes = match module_info_path(&bundle) {
            Some(info) => std::fs::read_to_string(&info)
                .map_err(anyhow::Error::from)
                .and_then(|text| parse_module_info(&text))
                .inspect_err(|e| eprintln!("vst3 : {} : {e:#}", info.display()))
                .ok(),
            None => {
                let mut cmd = Command::new(exe);
                cmd.arg(PROBE_ARG).arg(&bundle);
                run_probe(cmd, PROBE_TIMEOUT)
            }
        };
        match classes {
            Some(classes) => plugins.extend(plugins_of(&bundle, &classes)),
            None => eprintln!("vst3 : {} ignoré", bundle.display()),
        }
    }
    dedupe(plugins)
}

/// Lance `cmd`, attend au plus `timeout`, lit `Vec<ClassInfo>` en JSON sur sa sortie. Échec,
/// plantage ou dépassement (enfant tué) → `None`.
pub fn run_probe(mut cmd: Command, timeout: Duration) -> Option<Vec<ClassInfo>> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Lecture dans un thread : un enfant bavard ne doit pas se bloquer sur un tube plein. Le
    // résultat arrive par un canal attendu avec le reste du délai : un petit-enfant (démon lancé
    // par le plugin) qui garde le tube ouvert ne doit pas bloquer le scan — le thread est alors
    // abandonné.
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = tx.send(out);
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    if !status?.success() {
        return None;
    }
    let out = rx
        .recv_timeout(timeout.saturating_sub(started.elapsed()))
        .ok()?;
    serde_json::from_str(out.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_timeout_kills_child() {
        let cmd = if cfg!(windows) {
            let mut c = Command::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        let started = Instant::now();
        assert_eq!(run_probe(cmd, Duration::from_secs(1)), None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn probe_grandchild_holding_stdout_does_not_hang() {
        // Le plugin sondé lance un processus (démon de licence…) qui hérite du tube de sortie et
        // survit à l'enfant : le scan ne doit pas attendre sa fin.
        let cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", "/B", "ping", "-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", "sleep 30 &"]);
            c
        };
        let started = Instant::now();
        assert_eq!(run_probe(cmd, Duration::from_secs(1)), None);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn probe_output_is_parsed() {
        let json = r#"[{"cid":"AB","category":"Audio Module Class","name":"Amp","vendor":"V","subCategories":["Fx"]}]"#;
        let file = std::env::temp_dir().join(format!("chelou-probe-{}.json", std::process::id()));
        std::fs::write(&file, json).unwrap();
        let cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.arg("/C").arg("type").arg(&file);
            c
        } else {
            let mut c = Command::new("cat");
            c.arg(&file);
            c
        };
        let classes = run_probe(cmd, Duration::from_secs(10));
        std::fs::remove_file(&file).unwrap();
        let classes = classes.unwrap();
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "Amp");
    }

    #[test]
    fn failing_probe_gives_nothing() {
        let cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit", "3"]);
            c
        } else {
            Command::new("false")
        };
        assert_eq!(run_probe(cmd, Duration::from_secs(10)), None);
    }
}
