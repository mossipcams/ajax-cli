#![deny(unsafe_op_in_unsafe_fn)]

fn main() {
    match ajax_cli::run_with_args(std::env::args_os()) {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
