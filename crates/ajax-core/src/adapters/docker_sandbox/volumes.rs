use std::collections::{BTreeSet, HashSet};
use std::io;
use std::path::Path;
use std::process::Output;

use super::{pair, strings, succeeded, AJAX_SANDBOX_OFF};

const TARGET_PREFIX: &str = "ajax-target-";
const NODE_MODULES_PREFIX: &str = "ajax-node-modules-";

pub fn task_volumes(key: &str) -> [String; 2] {
    [
        format!("{TARGET_PREFIX}{key}"),
        format!("{NODE_MODULES_PREFIX}{key}"),
    ]
}

pub fn volume_rm_argv(volumes: &[String]) -> Vec<String> {
    let mut argv = strings(&["volume", "rm", "-f"]);
    argv.extend(volumes.iter().cloned());
    argv
}

pub fn volume_init_argv(image: &str, volume: &str, uid: u32, gid: u32) -> Vec<String> {
    let mut argv = strings(&["run", "--rm", "--user", "0"]);
    argv.extend(pair("-v", format!("{volume}:/v")));
    argv.push(image.to_string());
    argv.push("chown".to_string());
    argv.push(format!("{uid}:{gid}"));
    argv.push("/v".to_string());
    argv
}

pub fn volume_create_argv(server_id: &str, volume: &str) -> Vec<String> {
    let mut argv = strings(&["volume", "create"]);
    argv.extend(pair("--label", format!("ajax.server={server_id}")));
    argv.push(volume.to_string());
    argv
}

pub fn volume_ls_argv(server_id: &str) -> Vec<String> {
    let mut argv = strings(&["volume", "ls", "-q"]);
    argv.extend(pair("--filter", format!("label=ajax.server={server_id}")));
    argv
}

pub fn sweep_volume_rm_argv(volumes: &[String]) -> Vec<String> {
    let mut argv = strings(&["volume", "rm"]);
    argv.extend(volumes.iter().cloned());
    argv
}

fn volume_key(volume: &str) -> Option<&str> {
    volume
        .strip_prefix(TARGET_PREFIX)
        .or_else(|| volume.strip_prefix(NODE_MODULES_PREFIX))
}

pub fn sweep_volumes_to_remove(listing: &str, live_keys: &HashSet<String>) -> Vec<String> {
    let mut orphaned = BTreeSet::new();
    for name in listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if volume_key(name).is_some_and(|key| !live_keys.contains(key)) {
            orphaned.insert(name.to_string());
        }
    }
    orphaned.into_iter().collect()
}

pub fn sweep_orphan_task_volumes(docker: &Path, server_id: &str, live_keys: &HashSet<String>) {
    if cfg!(test) {
        return;
    }
    let Some(listing) = super::run_quietly(docker, &volume_ls_argv(server_id)) else {
        return;
    };
    for volume in sweep_volumes_to_remove(&listing, live_keys) {
        let _ = super::run_quietly(docker, &sweep_volume_rm_argv(std::slice::from_ref(&volume)));
    }
}

pub fn ensure_task_volumes(
    docker: &Path,
    image: &str,
    key: &str,
    server_id: &str,
    uid: u32,
    gid: u32,
    run: impl Fn(&Path, &[&str]) -> io::Result<Output>,
) -> Result<(), String> {
    for volume in task_volumes(key) {
        if succeeded(run(docker, &["volume", "inspect", volume.as_str()])) {
            continue;
        }
        let create = volume_create_argv(server_id, &volume);
        let create_args: Vec<&str> = create.iter().map(String::as_str).collect();
        if !succeeded(run(docker, &create_args)) {
            return Err(volume_error("create", &volume));
        }
        let init = volume_init_argv(image, &volume, uid, gid);
        let init_args: Vec<&str> = init.iter().map(String::as_str).collect();
        if !succeeded(run(docker, &init_args)) {
            let _ = run(docker, &["volume", "rm", "-f", volume.as_str()]);
            return Err(volume_error("chown", &volume));
        }
    }
    Ok(())
}

fn volume_error(step: &str, volume: &str) -> String {
    format!("agent sandbox could not {step} volume {volume}; {AJAX_SANDBOX_OFF}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const UID: u32 = 501;
    const GID: u32 = 20;
    const KEY: &str = "ajax-docker-sandboxes-1576734c";
    const IMAGE: &str = "ajax-agent:test";
    const SERVER_ID: &str = "0123456789abcdef";

    fn output(code: i32) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    type VolumeCalls = RefCell<Vec<Vec<String>>>;

    fn volume_recorder<'a>(
        calls: &'a VolumeCalls,
        respond: impl Fn(&[&str]) -> Output + 'a,
    ) -> impl Fn(&Path, &[&str]) -> io::Result<Output> + 'a {
        move |_, args| {
            calls
                .borrow_mut()
                .push(args.iter().map(|arg| arg.to_string()).collect());
            Ok(respond(args))
        }
    }

    fn ensure(run: impl Fn(&Path, &[&str]) -> io::Result<Output>) -> Result<(), String> {
        ensure_task_volumes(
            Path::new("/opt/docker"),
            IMAGE,
            KEY,
            SERVER_ID,
            UID,
            GID,
            run,
        )
    }

    fn target() -> String {
        format!("ajax-target-{KEY}")
    }

    fn node_modules() -> String {
        format!("ajax-node-modules-{KEY}")
    }

    #[test]
    fn volume_init_argv_is_exact() {
        assert_eq!(
            volume_init_argv(IMAGE, "ajax-target-x", 501, 20),
            strings(&[
                "run",
                "--rm",
                "--user",
                "0",
                "-v",
                "ajax-target-x:/v",
                IMAGE,
                "chown",
                "501:20",
                "/v",
            ])
        );
    }

    #[test]
    fn existing_task_volumes_are_left_alone() {
        let calls = VolumeCalls::default();

        ensure(volume_recorder(&calls, |_| output(0))).expect("existing volumes");

        assert_eq!(
            calls.into_inner(),
            vec![
                strings(&["volume", "inspect", &target()]),
                strings(&["volume", "inspect", &node_modules()]),
            ]
        );
    }

    #[test]
    fn missing_task_volumes_are_created_then_chowned_in_order() {
        let calls = VolumeCalls::default();

        ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "volume" && args[1] == "inspect" {
                1
            } else {
                0
            })
        }))
        .expect("missing volumes are created");

        let mut expected = Vec::new();
        for volume in [target(), node_modules()] {
            expected.push(strings(&["volume", "inspect", &volume]));
            expected.push(volume_create_argv(SERVER_ID, &volume));
            expected.push(volume_init_argv(IMAGE, &volume, UID, GID));
        }
        assert_eq!(calls.into_inner(), expected);
    }

    #[test]
    fn a_failed_volume_create_refuses_the_launch() {
        let calls = VolumeCalls::default();

        let error = ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "volume" { 1 } else { 0 })
        }))
        .expect_err("failed create refuses");

        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
        assert!(error.contains("create"), "{error}");
        assert_eq!(calls.into_inner().len(), 2);
    }

    #[test]
    fn a_failed_volume_chown_refuses_the_launch() {
        let calls = VolumeCalls::default();

        let error = ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "run" || args[1] == "inspect" {
                1
            } else {
                0
            })
        }))
        .expect_err("failed chown refuses");

        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
        assert!(error.contains("chown"), "{error}");
        assert_eq!(calls.into_inner().len(), 4);
    }

    #[test]
    fn a_failed_volume_chown_removes_the_half_initialised_volume() {
        let calls = VolumeCalls::default();

        ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "run" || args[1] == "inspect" {
                1
            } else {
                0
            })
        }))
        .expect_err("failed chown refuses");

        assert_eq!(
            calls.into_inner().last(),
            Some(&strings(&["volume", "rm", "-f", &target()]))
        );
    }

    fn keys(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }

    #[test]
    fn the_volume_create_argv_carries_the_server_label() {
        assert_eq!(
            volume_create_argv(SERVER_ID, "ajax-target-x"),
            strings(&[
                "volume",
                "create",
                "--label",
                "ajax.server=0123456789abcdef",
                "ajax-target-x"
            ])
        );
    }

    #[test]
    fn the_volume_ls_argv_filters_on_the_given_server_id() {
        assert_eq!(
            volume_ls_argv(SERVER_ID),
            strings(&[
                "volume",
                "ls",
                "-q",
                "--filter",
                "label=ajax.server=0123456789abcdef"
            ])
        );
    }

    #[test]
    fn live_task_volumes_are_kept_by_the_sweep_selection() {
        let listing = format!("{}\n{}\n", target(), node_modules());

        assert_eq!(
            sweep_volumes_to_remove(&listing, &keys(&[KEY])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn orphan_task_volumes_are_removed_by_the_sweep_selection() {
        let listing = format!("{}\n{}\n", target(), node_modules());

        assert_eq!(
            sweep_volumes_to_remove(&listing, &keys(&[])),
            vec![node_modules(), target()]
        );
    }

    #[test]
    fn the_sweep_selection_ignores_non_ajax_volume_names() {
        let listing = format!(
            "{}\nother-server-volume\nahome-thing\nhuman-data\n",
            target()
        );

        assert_eq!(
            sweep_volumes_to_remove(&listing, &keys(&[])),
            vec![target()]
        );
    }

    #[test]
    fn the_sweep_rm_argv_is_not_forced() {
        let volumes = vec![
            "ajax-target-x".to_string(),
            "ajax-node-modules-x".to_string(),
        ];

        assert_eq!(
            sweep_volume_rm_argv(&volumes),
            strings(&["volume", "rm", "ajax-target-x", "ajax-node-modules-x"])
        );
    }
}
