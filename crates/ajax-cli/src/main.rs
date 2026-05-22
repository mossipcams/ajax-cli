#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{self, Write};

fn main() {
    match ajax_cli::run_with_args(expand_ajax_cli_args(std::env::args_os())) {
        Ok(output) => {
            if let Err(error) = write_process_output(&mut io::stdout().lock(), &output) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn expand_ajax_cli_args<I, T>(args: I) -> Vec<std::ffi::OsString>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    let mut args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    match args.get(1).and_then(|arg| arg.to_str()) {
        None => {
            args.push("cockpit".into());
        }
        Some("dev") => {
            args.remove(1);
            args.insert(1, "--profile".into());
            args.insert(2, "dev".into());
            args.insert(3, "cockpit".into());
        }
        Some(_) => {}
    }

    args
}

fn write_process_output(writer: &mut impl Write, output: &str) -> io::Result<()> {
    match writeln!(writer, "{output}") {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::{expand_ajax_cli_args, write_process_output};
    use std::io;

    struct BrokenPipeWriter;

    impl io::Write for BrokenPipeWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed pipe"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn output_writer_treats_broken_pipe_as_success() {
        let result = write_process_output(&mut BrokenPipeWriter, "hello");

        assert!(result.is_ok());
    }

    #[test]
    fn output_writer_preserves_trailing_newline() {
        let mut output = Vec::new();

        write_process_output(&mut output, "hello").unwrap();

        assert_eq!(output, b"hello\n");
    }

    struct FailingWriter;

    impl io::Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("disk full"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn output_writer_surfaces_non_broken_pipe_errors() {
        let error = write_process_output(&mut FailingWriter, "hello").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn bare_ajax_cli_runs_stable_cockpit() {
        let args = expand_ajax_cli_args(["ajax-cli"]);

        assert_eq!(args, ["ajax-cli", "cockpit"]);
    }

    #[test]
    fn ajax_cli_dev_runs_dev_profile_cockpit() {
        let args = expand_ajax_cli_args(["ajax-cli", "dev"]);

        assert_eq!(args, ["ajax-cli", "--profile", "dev", "cockpit"]);
    }

    #[test]
    fn ajax_cli_dev_preserves_cockpit_options() {
        let args = expand_ajax_cli_args(["ajax-cli", "dev", "--watch"]);

        assert_eq!(args, ["ajax-cli", "--profile", "dev", "cockpit", "--watch"]);
    }
}
