use std::fs::File;
use std::io::Write;
use std::os::fd::FromRawFd;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

fn run(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_checkrush"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn run_with_terminal(args: &[&str]) -> Output {
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: Both pointers are valid, and optional arguments are null.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(
        result,
        0,
        "openpty failed: {}",
        std::io::Error::last_os_error()
    );
    // SAFETY: openpty returned new, owned file descriptors.
    let _master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    let mut child = Command::new(env!("CARGO_BIN_EXE_checkrush"))
        .args(args)
        .stdin(Stdio::from(slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("checkrush waited for terminal input with arguments {args:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn terminal_input_shows_usage_without_waiting() {
    let expected = text(&run(&["--help"], ""));
    for args in [&[][..], &["--no-timer"][..], &["-v"][..]] {
        let output = run_with_terminal(args);
        assert!(output.status.success());
        assert_eq!(text(&output), expected);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn terminal_input_still_rejects_invalid_options() {
    let output = run_with_terminal(&["--invalid"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).contains("Unknown argument: --invalid"));
}

#[test]
fn redirected_file_runs_commands() {
    let path = std::env::temp_dir().join(format!(
        "checkrush-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, "printf hello # file-check\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_checkrush"))
        .arg("--no-timer")
        .stdin(Stdio::from(File::open(&path).unwrap()))
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let stdout = text(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("[file-check] succeeded"), "{stdout}");
}

#[test]
fn redirected_empty_file_is_not_treated_as_a_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_checkrush"))
        .stdin(Stdio::from(File::open("/dev/null").unwrap()))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(text(&output), "no commands provided\n");
}

#[test]
fn runs_named_commands_and_hides_successful_output() {
    let output = run(
        &["--no-timer"],
        "printf secret # first\nprintf other # second\n\n",
    );
    let stdout = text(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("🚀 [first] started"), "{stdout}");
    assert!(stdout.contains("✅ [first] succeeded"), "{stdout}");
    assert!(stdout.contains("✅ [second] succeeded"), "{stdout}");
    assert!(stdout.contains("all passed in"), "{stdout}");
    assert!(!stdout.contains("secret"), "{stdout}");
    assert!(!stdout.contains("other"), "{stdout}");
    assert!(!stdout.contains("\x1b[K"), "{stdout}");
}

#[test]
fn reports_failure_output_and_cancels_other_processes() {
    let start = Instant::now();
    let output = run(
        &["--no-timer"],
        "sleep 0.1; printf failure-out; printf failure-err >&2; exit 5 # bad\nsleep 5; echo should-not-appear # slow\n",
    );
    let stdout = text(&output);
    assert_eq!(output.status.code(), Some(5), "{stdout}");
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "took {:?}",
        start.elapsed()
    );
    assert!(stdout.contains("[slow] cancelled"), "{stdout}");
    assert!(stdout.contains("❌ [bad] failed"), "{stdout}");
    assert!(
        stdout.contains("Command: sleep 0.1; printf failure-out; printf failure-err >&2; exit 5"),
        "{stdout}"
    );
    assert!(stdout.contains("Exit code: 5"), "{stdout}");
    assert!(stdout.contains("--- stdout ---\nfailure-out"), "{stdout}");
    assert!(stdout.contains("--- stderr ---\nfailure-err"), "{stdout}");
    assert!(!stdout.contains("should-not-appear"), "{stdout}");
}

#[test]
fn verbose_prints_prefixed_output_from_both_streams() {
    let output = run(
        &["--no-timer", "--verbose"],
        "echo hello; echo problem >&2 # example\n",
    );
    let stdout = text(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("[example] hello"), "{stdout}");
    assert!(stdout.contains("[example] E problem"), "{stdout}");
}

#[test]
fn verbose_failure_prints_output_once() {
    let output = run(&["--no-timer", "-v"], "echo once; exit 7 # bad\n");
    let stdout = text(&output);
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(stdout.matches("[bad] once").count(), 1, "{stdout}");
    assert!(!stdout.contains("--- stdout ---"), "{stdout}");
}

#[test]
fn default_name_and_literal_hash_work() {
    let output = run(&["--no-timer", "-v"], "printf '%s' '#value'\n");
    let stdout = text(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("[printf '%s' '#value'] #value"), "{stdout}");
}

#[test]
fn empty_input_succeeds() {
    let output = run(&["--no-timer"], "  \n\n");
    assert!(output.status.success());
    assert_eq!(text(&output), "no commands provided\n");
}

#[test]
fn rejects_unknown_and_duplicate_options() {
    for args in [&["--invalid"][..], &["--no-timer", "--no-timer"][..]] {
        let output = run(args, "");
        assert_eq!(output.status.code(), Some(2));
        assert!(text(&output).contains("Usage: checkrush"));
    }
}

#[test]
fn timer_is_enabled_by_default() {
    let output = run(&[], "sleep 0.2 # wait\n");
    let stdout = text(&output);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("elapsed:"), "{stdout}");
    assert!(stdout.contains("\x1b[K"), "{stdout}");
}
