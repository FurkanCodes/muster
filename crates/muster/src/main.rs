//! The `muster` command. With no arguments it will open the terminal UI; its
//! subcommands start the daemon and drive it from scripts.

fn main() {
    println!("muster {}", env!("CARGO_PKG_VERSION"));
}
