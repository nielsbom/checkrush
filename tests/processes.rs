use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn start(input: &str) -> std::process::Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_checkrush"))
        .arg("--no-timer")
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

#[test]
fn escalates_when_a_process_ignores_sigterm() {
    let start_time = Instant::now();
    let output = start("sleep 0.3; exit 5 # bad\ntrap '' TERM; sleep 5 # stubborn\n")
        .wait_with_output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(start_time.elapsed() < Duration::from_secs(3));
    assert!(String::from_utf8_lossy(&output.stdout).contains("[stubborn] cancelled"));
}

#[test]
fn sigterm_stops_running_commands() {
    let child = start("sleep 5 # slow\n");
    thread::sleep(Duration::from_millis(300));
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let start_time = Instant::now();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(143));
    assert!(start_time.elapsed() < Duration::from_secs(3));
}
