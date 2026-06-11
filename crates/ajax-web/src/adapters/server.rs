//! Web Cockpit process lifecycle (restart via re-exec).

#[cfg(not(test))]
use std::{process::Command, thread, time::Duration};

#[cfg(not(test))]
const RESTART_DELAY: Duration = Duration::from_millis(400);

/// Re-exec the current process with the same argv after a short delay, then exit.
///
/// Under `cfg(test)` this is a no-op so integration tests do not terminate the runner.
pub fn schedule_process_restart() {
    #[cfg(not(test))]
    {
        thread::spawn(|| {
            thread::sleep(RESTART_DELAY);
            if let Err(error) = respawn_current_process() {
                eprintln!("Ajax web restart failed: {error}");
            }
            std::process::exit(0);
        });
    }
}

#[cfg(not(test))]
fn respawn_current_process() -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not resolve executable: {error}"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    Command::new(&executable)
        .args(args)
        .envs(std::env::vars())
        .spawn()
        .map_err(|error| format!("could not spawn replacement process: {error}"))?;
    Ok(())
}
