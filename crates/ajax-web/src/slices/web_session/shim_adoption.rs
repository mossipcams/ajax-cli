use std::path::{Path, PathBuf};

use ajax_core::registry::{Registry, SqliteRegistryStore};

use crate::adapters::session_shim::spawn::probe_live;
use crate::adapters::web_session_store::prompt_ledger::{self as ledger_module, PromptPhase};

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Adoptable {
    pub worktree: PathBuf,
    pub client_message_id: String,
    pub shim_offset: u64,
    pub transcript_cursor: u64,
}

pub(crate) fn adoptable(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
) -> Option<Adoptable> {
    let worktree = store
        .load_tasks_only()
        .ok()?
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == handle)?
        .worktree_path
        .clone();
    let ledger = ledger_module::try_load(store, state_dir, handle).ok()?;
    let (client_message_id, shim_offset, transcript_cursor) =
        ledger.entries.iter().find_map(|entry| {
            match (entry.phase, entry.shim_offset, entry.transcript_cursor) {
                (PromptPhase::Dispatching, Some(shim_offset), Some(transcript_cursor)) => Some((
                    entry.client_message_id.clone(),
                    shim_offset,
                    transcript_cursor,
                )),
                _ => None,
            }
        })?;
    if !probe_live(state_dir, &worktree) {
        return None;
    }
    Some(Adoptable {
        worktree,
        client_message_id,
        shim_offset,
        transcript_cursor,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use ajax_core::registry::SqliteRegistryStore;

    use super::adoptable;
    use crate::adapters::session_shim::shim_paths;
    use crate::adapters::session_shim::spawn::{probe_live, spawn_test_shim};
    use crate::adapters::web_session_store::prompt_ledger::{self, PromptLedger, PromptPhase};
    use crate::slices::web_session::orphan_sweep::sweep_adopting;
    use crate::slices::web_session::os_process;
    use crate::slices::web_session::process_identity::start_token;
    use crate::slices::web_session::startup_recovery::recover_interrupted_prompts;

    const SCRATCH_ROOT: &str = "../../target/web-adopt-tests";
    const INTERRUPTED_NOTE: &str = "was interrupted by host restart";

    struct LiveShim {
        state_dir: PathBuf,
        worktree: String,
        serve: Option<JoinHandle<io::Result<i32>>>,
    }

    impl LiveShim {
        fn start(state_dir: &Path, worktree: &str) -> Self {
            let serve = spawn_test_shim(state_dir, worktree);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !probe_live(state_dir, Path::new(worktree)) {
                assert!(Instant::now() < deadline, "shim never listened");
                thread::sleep(Duration::from_millis(10));
            }
            Self {
                state_dir: state_dir.to_path_buf(),
                worktree: worktree.to_owned(),
                serve: Some(serve),
            }
        }
    }

    impl Drop for LiveShim {
        fn drop(&mut self) {
            let Some(serve) = self.serve.take() else {
                return;
            };
            let socket = shim_paths(&self.state_dir, &self.worktree).socket;
            if let Ok(mut stream) = UnixStream::connect(socket) {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
                if stream.write_all(b"REPLAY 0\nSHUTDOWN\n").is_ok() {
                    let _ = stream.read_to_end(&mut Vec::new());
                }
            }
            let _ = serve.join();
        }
    }

    struct Scratch {
        dir: PathBuf,
        store: SqliteRegistryStore,
        handle: String,
        worktree: String,
    }

    fn scratch(label: &str) -> Scratch {
        let dir = PathBuf::from(SCRATCH_ROOT).join(format!("{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let store = SqliteRegistryStore::new(dir.join("registry.db"));
        let worktree = dir
            .join("worktree")
            .to_str()
            .expect("utf-8 path")
            .to_owned();
        let mut task = crate::test_support::fix_login_task();
        task.worktree_path = PathBuf::from(&worktree);
        let handle = task.qualified_handle();
        let registry = crate::test_support::context_with_tasks(&["web"], vec![task]).registry;
        store.save(&registry).expect("seed registry");
        Scratch {
            dir,
            store,
            handle,
            worktree,
        }
    }

    struct SleepChild {
        pid: i64,
        reaper: Option<JoinHandle<ExitStatus>>,
    }

    impl SleepChild {
        fn spawn() -> Self {
            let child: Child = Command::new("sleep")
                .arg("60")
                .stdin(Stdio::null())
                .spawn()
                .expect("spawn sleep helper");
            let pid = i64::from(child.id());
            let reaper = thread::spawn(move || {
                let mut child = child;
                child.wait().expect("wait on sleep helper")
            });
            Self {
                pid,
                reaper: Some(reaper),
            }
        }

        fn alive(&self) -> bool {
            os_process::exists(self.pid) && !os_process::is_zombie(self.pid)
        }
    }

    impl Drop for SleepChild {
        fn drop(&mut self) {
            let _ = Command::new("kill")
                .args(["-KILL", &self.pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Some(reaper) = self.reaper.take() {
                let _ = reaper.join();
            }
        }
    }

    fn seed_foreign_run(scratch: &Scratch, child: &SleepChild) {
        let token = start_token(child.pid).expect("live child token");
        crate::adapters::web_session_store::record_child_identity(
            &scratch.store,
            &scratch.dir,
            &scratch.handle,
            "acp",
            Some((child.pid, token)),
            "foreign-boot",
        )
        .expect("record child identity");
    }

    fn persist_dispatching(scratch: &Scratch, offsets: Option<(u64, u64)>) {
        let mut ledger = PromptLedger::default();
        ledger.upsert_queued(
            "p1".to_owned(),
            "transcript p1".to_owned(),
            "prompt p1".to_owned(),
            Vec::new(),
        );
        assert!(ledger.mark_dispatching("p1"));
        let (shim_offset, cursor) =
            offsets.map_or((None, 0), |(offset, cursor)| (Some(offset), cursor));
        ledger.record_dispatch_marks("p1", shim_offset, cursor);
        prompt_ledger::try_persist(&scratch.store, &scratch.dir, &scratch.handle, &ledger)
            .expect("persist ledger");
    }

    fn interruption_notes(scratch: &Scratch) -> usize {
        let session = crate::adapters::web_session_store::try_load::<serde_json::Value>(
            &scratch.store,
            &scratch.dir,
            &scratch.handle,
            "acp",
        )
        .expect("load transcript");
        session
            .events
            .iter()
            .filter(|event| {
                serde_json::to_string(event).is_ok_and(|text| text.contains(INTERRUPTED_NOTE))
            })
            .count()
    }

    fn phase_of_p1(scratch: &Scratch) -> PromptPhase {
        prompt_ledger::try_load(&scratch.store, &scratch.dir, &scratch.handle)
            .expect("load ledger")
            .entry("p1")
            .expect("p1 exists")
            .phase
    }

    #[test]
    fn adoptable_is_none_without_a_live_shim() {
        let scratch = scratch("adoptable-none");
        persist_dispatching(&scratch, Some((0, 0)));
        assert!(adoptable(&scratch.store, &scratch.dir, &scratch.handle).is_none());
        let _ = std::fs::remove_dir_all(&scratch.dir);
    }

    #[test]
    fn adoptable_reports_the_dispatching_prompt_and_offsets() {
        let scratch = scratch("adoptable-fields");
        let shim = LiveShim::start(&scratch.dir, &scratch.worktree);
        persist_dispatching(&scratch, Some((7, 3)));

        let found = adoptable(&scratch.store, &scratch.dir, &scratch.handle).expect("adoptable");
        assert_eq!(found.worktree, PathBuf::from(&scratch.worktree));
        assert_eq!(found.client_message_id, "p1");
        assert_eq!((found.shim_offset, found.transcript_cursor), (7, 3));

        drop(shim);
        let _ = std::fs::remove_dir_all(&scratch.dir);
    }

    #[test]
    fn live_adoptable_shim_is_left_running_by_recovery_and_sweep() {
        let scratch = scratch("adopt-live");
        let shim = LiveShim::start(&scratch.dir, &scratch.worktree);
        let child = SleepChild::spawn();
        seed_foreign_run(&scratch, &child);
        persist_dispatching(&scratch, Some((0, 0)));

        let sweep = sweep_adopting(
            &scratch.store,
            &scratch.dir,
            "this-boot",
            Duration::from_secs(5),
        );
        assert!(sweep.complete);
        assert!(child.alive(), "sweep must not kill an adoptable shim");

        let recovered = recover_interrupted_prompts(&scratch.store, &scratch.dir, "this-boot");
        assert!(recovered.complete);
        assert!(recovered.recovered_handles.is_empty());
        assert_eq!(phase_of_p1(&scratch), PromptPhase::Dispatching);
        assert_eq!(interruption_notes(&scratch), 0);
        assert!(probe_live(&scratch.dir, Path::new(&scratch.worktree)));

        drop(shim);
        drop(child);
        let _ = std::fs::remove_dir_all(&scratch.dir);
    }

    #[test]
    fn live_shim_without_offsets_is_interrupted_and_swept_as_before() {
        let scratch = scratch("no-offsets");
        let shim = LiveShim::start(&scratch.dir, &scratch.worktree);
        let child = SleepChild::spawn();
        seed_foreign_run(&scratch, &child);
        persist_dispatching(&scratch, None);

        let sweep = sweep_adopting(
            &scratch.store,
            &scratch.dir,
            "this-boot",
            Duration::from_secs(5),
        );
        assert!(sweep.complete);
        assert!(!child.alive(), "non-adoptable shim child must be swept");

        let recovered = recover_interrupted_prompts(&scratch.store, &scratch.dir, "this-boot");
        assert!(recovered.complete);
        assert_eq!(recovered.recovered_handles, vec![scratch.handle.clone()]);
        assert_eq!(phase_of_p1(&scratch), PromptPhase::Interrupted);
        assert_eq!(interruption_notes(&scratch), 1);

        drop(shim);
        let _ = std::fs::remove_dir_all(&scratch.dir);
    }
}
