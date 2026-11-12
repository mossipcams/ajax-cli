use std::{
    env, fs, io,
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use ajax_core::adapters::{
    docker_sandbox::{build_argv, image_tag, resolve_docker, DOCKERFILE},
    sandbox::sandbox_enabled,
};
use nix::unistd::{getgid, getuid};

use super::program::global_npm_root;

const CONTEXT_DIR: &str = "ajax-sandbox-image-context";
const PI_ACP: &str = "pi-acp";
const STDERR_TAIL_LINES: usize = 12;

static BUILD_STARTED: AtomicBool = AtomicBool::new(false);

pub(crate) struct ImagePlan<'a> {
    pub(crate) tag: &'a str,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) home: &'a Path,
    pub(crate) context: &'a Path,
    pub(crate) pi_acp: fn() -> Option<PathBuf>,
}

pub fn ensure_image_in_background() {
    if !sandbox_enabled() || BUILD_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = thread::Builder::new()
        .name("ajax-sandbox-image".to_string())
        .spawn(build_missing_image);
    if let Err(error) = spawned {
        tracing::warn!(target: "ajax_web", error = %error, "sandbox image build thread did not start");
    }
}

fn build_missing_image() {
    let Ok(docker) = resolve_docker() else {
        return;
    };
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let tag = image_tag();
    let context = env::temp_dir().join(format!("{CONTEXT_DIR}-{}", std::process::id()));
    let plan = ImagePlan {
        tag: &tag,
        uid: getuid().as_raw(),
        gid: getgid().as_raw(),
        home: &home,
        context: &context,
        pi_acp: || global_npm_root().map(|root| root.join(PI_ACP)),
    };
    if let Err(error) = ensure_image(&docker, &plan, run_docker) {
        tracing::warn!(target: "ajax_web", error = %error, "sandbox image build failed");
    }
}

pub(crate) fn ensure_image(
    docker: &Path,
    plan: &ImagePlan<'_>,
    run: impl Fn(&Path, &[&str], Option<&[u8]>) -> io::Result<Output>,
) -> Result<(), String> {
    if !succeeded(run(
        docker,
        &["info", "--format", "{{.ServerVersion}}"],
        None,
    )) {
        return Ok(());
    }
    if succeeded(run(docker, &["image", "inspect", plan.tag], None)) {
        return Ok(());
    }
    tracing::info!(target: "ajax_web", tag = plan.tag, "building missing sandbox image");
    let pi_acp = (plan.pi_acp)();
    let built = prepare_context(plan.context, pi_acp.as_deref())
        .map_err(|error| format!("sandbox image context could not be prepared: {error}"))
        .and_then(|()| {
            let argv = build_argv(plan.tag, plan.uid, plan.gid, plan.home, plan.context);
            run(docker, &refs(&argv), Some(DOCKERFILE.as_bytes()))
                .map_err(|error| format!("sandbox image build could not start: {error}"))
        });
    let _ = fs::remove_dir_all(plan.context);
    let output = built?;
    if output.status.success() {
        tracing::info!(target: "ajax_web", tag = plan.tag, "sandbox image built");
        return Ok(());
    }
    Err(format!(
        "sandbox image build failed: {}",
        stderr_tail(&output.stderr)
    ))
}

fn prepare_context(context: &Path, pi_acp: Option<&Path>) -> io::Result<()> {
    if let Err(error) = fs::remove_dir_all(context) {
        if error.kind() != io::ErrorKind::NotFound {
            return Err(error);
        }
    }
    fs::DirBuilder::new().mode(0o700).create(context)?;
    if let Some(source) = pi_acp.filter(|source| source.exists()) {
        let status = Command::new("cp")
            .arg("-a")
            .arg(source)
            .arg(context.join(PI_ACP))
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "cp -a {} failed",
                source.display()
            )));
        }
    }
    Ok(())
}

fn run_docker(docker: &Path, args: &[&str], stdin: Option<&[u8]>) -> io::Result<Output> {
    let mut command = Command::new(docker);
    command
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(bytes)?;
    }
    child.wait_with_output()
}

fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(STDERR_TAIL_LINES);
    lines[start..].join("\n")
}

fn succeeded(result: io::Result<Output>) -> bool {
    result.is_ok_and(|output| output.status.success())
}

fn refs(argv: &[String]) -> Vec<&str> {
    argv.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const TAG: &str = "ajax-agent:test";
    const UID: u32 = 501;
    const GID: u32 = 20;
    const HOME: &str = "/ajax-test-home";

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_string()).collect()
    }

    type Calls = RefCell<Vec<(Vec<String>, Option<Vec<u8>>)>>;

    fn output(code: i32) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/sandbox-tests/sandbox-image")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn image_recorder<'a>(
        calls: &'a Calls,
        respond: impl Fn(&[&str]) -> Output + 'a,
    ) -> impl Fn(&Path, &[&str], Option<&[u8]>) -> io::Result<Output> + 'a {
        move |_, args, stdin| {
            calls.borrow_mut().push((
                args.iter().map(|arg| arg.to_string()).collect(),
                stdin.map(<[u8]>::to_vec),
            ));
            Ok(respond(args))
        }
    }

    fn plan<'a>(home: &'a Path, context: &'a Path) -> ImagePlan<'a> {
        ImagePlan {
            tag: TAG,
            uid: UID,
            gid: GID,
            home,
            context,
            pi_acp: || None,
        }
    }

    #[test]
    fn a_present_image_is_neither_rebuilt_nor_given_a_context() {
        let dir = scratch_dir("present");
        let context = dir.join("context");
        let calls = Calls::default();
        let run = image_recorder(&calls, |_| output(0));

        ensure_image(
            Path::new("/opt/docker"),
            &plan(Path::new(HOME), &context),
            run,
        )
        .expect("present image is not an error");

        let calls = calls.into_inner();
        assert_eq!(
            calls,
            vec![
                (argv(&["info", "--format", "{{.ServerVersion}}"]), None),
                (argv(&["image", "inspect", TAG]), None),
            ]
        );
        assert!(!context.exists());
    }

    #[test]
    fn an_unreachable_daemon_builds_nothing_and_is_not_an_error() {
        let dir = scratch_dir("daemon-down");
        let context = dir.join("context");
        let calls = Calls::default();
        let run = image_recorder(&calls, |_| output(1));

        ensure_image(
            Path::new("/opt/docker"),
            &plan(Path::new(HOME), &context),
            run,
        )
        .expect("unreachable daemon is quiet");

        let calls = calls.into_inner();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0[0], "info");
        assert!(!context.exists());
    }

    #[test]
    fn a_missing_image_is_built_from_the_embedded_dockerfile_on_stdin() {
        let dir = scratch_dir("missing");
        let context = dir.join("context");
        let calls = Calls::default();
        let run = image_recorder(&calls, |args| {
            output(if args[0] == "image" { 1 } else { 0 })
        });

        ensure_image(
            Path::new("/opt/docker"),
            &plan(Path::new(HOME), &context),
            run,
        )
        .expect("successful build");

        let calls = calls.into_inner();
        assert_eq!(calls.len(), 3);
        let expected = build_argv(TAG, UID, GID, Path::new(HOME), &context);
        assert_eq!(calls[2].0, expected);
        assert_eq!(calls[2].1.as_deref(), Some(DOCKERFILE.as_bytes()));
        assert!(!context.exists());
    }

    #[test]
    fn a_failed_build_reports_the_tail_of_stderr() {
        let dir = scratch_dir("build-fails");
        let context = dir.join("context");
        let calls = Calls::default();
        let run = image_recorder(&calls, |args| {
            if args[0] == "build" {
                Output {
                    status: ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: b"step one\nE: unable to fetch\n".to_vec(),
                }
            } else {
                output(if args[0] == "image" { 1 } else { 0 })
            }
        });

        let error = ensure_image(
            Path::new("/opt/docker"),
            &plan(Path::new(HOME), &context),
            run,
        )
        .expect_err("failed build is an error");

        assert!(error.contains("E: unable to fetch"), "{error}");
    }

    #[test]
    fn the_build_context_is_private_during_the_build_and_removed_after() {
        let dir = scratch_dir("private-context");
        let context = dir.join("context");
        let calls = Calls::default();
        let recorded = image_recorder(&calls, |args| {
            output(if args[0] == "image" { 1 } else { 0 })
        });
        let mode = Cell::new(None);
        let run = |docker: &Path, args: &[&str], stdin: Option<&[u8]>| {
            if args[0] == "build" {
                let metadata = fs::metadata(&context).expect("context exists during build");
                mode.set(Some(metadata.permissions().mode() & 0o777));
            }
            recorded(docker, args, stdin)
        };

        ensure_image(
            Path::new("/opt/docker"),
            &plan(Path::new(HOME), &context),
            run,
        )
        .expect("successful build");

        assert_eq!(mode.get(), Some(0o700));
        assert!(!context.exists());
    }

    #[test]
    fn the_context_carries_pi_acp_only_when_its_source_exists() {
        let dir = scratch_dir("context");
        let source = dir.join("npm/pi-acp");
        fs::create_dir_all(source.join("dist")).unwrap();
        fs::write(source.join("dist/index.js"), "export {};").unwrap();

        let with_source = dir.join("with-source");
        let stale = with_source.join("stale.txt");
        fs::create_dir_all(&with_source).unwrap();
        fs::write(&stale, "old").unwrap();
        prepare_context(&with_source, Some(&source)).expect("context with source");
        assert!(with_source.join("pi-acp/dist/index.js").is_file());
        assert!(!stale.exists());

        let missing = dir.join("missing-source");
        prepare_context(&missing, Some(&dir.join("npm/absent"))).expect("context without source");
        assert!(missing.is_dir());
        assert!(!missing.join("pi-acp").exists());

        let none = dir.join("no-source");
        prepare_context(&none, None).expect("context without any source");
        assert!(!none.join("pi-acp").exists());
    }
}
