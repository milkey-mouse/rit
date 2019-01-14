extern crate rit_launcher;

use rit_launcher::RitLauncher;
use std::process;

fn main() {
    if let Err(e) = rit_launcher::get_default_launcher().launch("status", &[]) {
        eprintln!("error: {}", e);
        process::exit(1);
    }
}
