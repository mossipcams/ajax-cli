use super::{send_command, TaskSessionCommand, TaskSessionDirectory};

impl TaskSessionDirectory {
    pub async fn flush_all(&self) {
        let senders: Vec<_> = {
            let sessions = self.sessions.lock().unwrap();
            sessions
                .values()
                .map(|entry| entry.command_tx.clone())
                .collect()
        };
        futures::future::join_all(
            senders
                .iter()
                .map(|tx| send_command(tx, |reply| TaskSessionCommand::Flush { reply })),
        )
        .await;
    }
}
