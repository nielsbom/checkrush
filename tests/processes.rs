use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

struct ReadyFile {
    directory: PathBuf,
    path: PathBuf,
}

impl ReadyFile {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "checkrush-ready-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("ready");
        Self { directory, path }
    }
}

impl Drop for ReadyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

fn start(input: &str) -> std::process::Child {
    start_with_ready(input, None)
}

fn start_with_ready(input: &str, ready: Option<&Path>) -> std::process::Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_checkrush"));
    command
        .arg("--no-timer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(ready) = ready {
        command.env("CHECKRUSH_TEST_READY", ready);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child
}

#[test]
fn detects_failure_when_a_descendant_holds_output_open() {
    let start_time = Instant::now();
    let output = start("sleep 5 & echo failed; exit 5 # broken\nsleep 5 # slow\n")
        .wait_with_output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(start_time.elapsed() < Duration::from_secs(3));
    assert!(String::from_utf8_lossy(&output.stdout).contains("failed"));
}

fn start_after_ready(command: &str, ready: &Path) -> std::process::Child {
    let input = format!(
        "i=0; while [ ! -f \"$CHECKRUSH_TEST_READY\" ] && [ \"$i\" -lt 400 ]; do i=$((i + 1)); sleep 0.01; done; [ -f \"$CHECKRUSH_TEST_READY\" ] || exit 99; exit 5 # bad\n{command} # stubborn\n"
    );
    start_with_ready(&input, Some(ready))
}

#[test]
fn escalates_when_a_process_ignores_sigterm() {
    let ready = ReadyFile::new();
    let start_time = Instant::now();
    let output = start_after_ready(
        "trap '' TERM; : > \"$CHECKRUSH_TEST_READY\"; sleep 5",
        &ready.path,
    )
    .wait_with_output()
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(5), "{stdout}");
    assert!(start_time.elapsed() < Duration::from_secs(4));
    assert!(stdout.contains("[stubborn] cancelled"), "{stdout}");
}

#[test]
fn reports_cancellation_when_a_process_exits_with_a_code_after_sigterm() {
    let ready = ReadyFile::new();
    let output = start_after_ready(
        "trap 'exit 143' TERM; : > \"$CHECKRUSH_TEST_READY\"; while :; do sleep 0.1; done",
        &ready.path,
    )
    .wait_with_output()
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(5), "{stdout}");
    assert!(stdout.contains("[stubborn] cancelled"), "{stdout}");
}

#[test]
fn sigterm_stops_running_commands() {
    let mut child = start("sleep 5 # slow\n");
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut started = String::new();
    stdout.read_line(&mut started).unwrap();
    assert!(started.contains("[slow] started"), "{started}");
    // SAFETY: The child PID was returned by spawn and has not been reaped.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    let start_time = Instant::now();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(143));
    assert!(start_time.elapsed() < Duration::from_secs(3));
}
