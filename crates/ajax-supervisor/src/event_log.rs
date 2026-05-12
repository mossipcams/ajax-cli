use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use ajax_core::events::MonitorEvent;

use crate::SupervisorError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventLog {
    path: PathBuf,
}

impl EventLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, event: &MonitorEvent) -> Result<(), SupervisorError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                SupervisorError::Io(format!(
                    "failed to create event log directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| {
                SupervisorError::Io(format!(
                    "failed to open event log {}: {error}",
                    self.path.display()
                ))
            })?;
        let line = serde_json::to_string(event)?;
        writeln!(file, "{line}").map_err(|error| {
            SupervisorError::Io(format!(
                "failed to write event log {}: {error}",
                self.path.display()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use ajax_core::events::{MonitorEvent, ProcessEvent};

    use super::EventLog;

    #[test]
    fn event_log_appends_monitor_events_as_json_lines() {
        let root = std::env::temp_dir().join(format!("ajax-event-log-{}", std::process::id()));
        let path = root.join("events.jsonl");
        let log = EventLog::new(&path);

        log.append(&MonitorEvent::Process(ProcessEvent::Started {
            pid: Some(7),
        }))
        .unwrap();
        log.append(&MonitorEvent::Process(ProcessEvent::Hung {
            quiet_for: Duration::from_secs(3),
        }))
        .unwrap();

        let contents = fs::read_to_string(&path).unwrap();

        assert_eq!(contents.lines().count(), 2);
        assert!(contents.contains(r#""Started":{"pid":7}"#));
        assert!(contents.contains(r#""Hung":{"quiet_for":{"secs":3,"nanos":0}}"#));

        let _ = fs::remove_dir_all(root);
    }
}
