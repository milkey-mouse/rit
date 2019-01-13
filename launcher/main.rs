use std::process;

use rit_launcher;

fn main() {
  if let Err(e) = rit_launcher::run() {
    eprintln!("error: {}", e);
    process::exit(1);
  }
}
