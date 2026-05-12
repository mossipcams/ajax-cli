#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{self, Write};

fn main() {
    match ajax_cli::run_with_args(std::env::args_os()) {
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

fn write_process_output(writer: &mut impl Write, output: &str) -> io::Result<()> {
    match writeln!(writer, "{output}") {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::write_process_output;
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
}
