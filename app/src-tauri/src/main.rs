// Pas de console sous Windows en version finale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // L'exécutable sert aussi de crochet « avant outil » pour les agents (politique lecture seule).
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "policy-hook").unwrap_or(false) {
        let mut input = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
        let policy = args.get(2).map(std::path::PathBuf::from).unwrap_or_default();
        std::process::exit(steno_core::policy::hook_main(&policy, &input));
    }
    steno_app_lib::run()
}
