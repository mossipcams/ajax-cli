use super::{SessionEntry, TaskSessionCommand, TaskSessionDirectory};

impl TaskSessionDirectory {
    pub async fn detach_all(&self) {
        let entries: Vec<SessionEntry> = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.drain().map(|(_, entry)| entry).collect()
        };
        futures::future::join_all(entries.into_iter().map(|entry| async move {
            let _ = entry
                .command_tx
                .send(TaskSessionCommand::Shutdown { close: false })
                .await;
            let _ = entry.join_handle.await;
        }))
        .await;
    }
}
