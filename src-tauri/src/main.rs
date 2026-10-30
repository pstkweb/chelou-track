#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // Processus enfant du scan VST3 : imprime les classes d'un bundle et sort, sans lancer l'app.
    if args
        .get(1)
        .is_some_and(|a| a == chelou_vst3_host::scan::PROBE_ARG)
    {
        std::process::exit(chelou_track_lib::vst3_probe(
            args.get(2).map(|a| a.as_os_str()),
        ));
    }
    chelou_track_lib::run();
}
