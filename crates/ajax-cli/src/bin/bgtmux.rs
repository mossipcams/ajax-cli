#![deny(unsafe_op_in_unsafe_fn)]

use std::io;

fn main() {
    match ajax_cli::run_bgtmux_to_writer(std::env::args_os(), &mut io::stdout().lock()) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
