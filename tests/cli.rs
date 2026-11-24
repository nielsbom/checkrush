use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_checkrush"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
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
    assert!(start.elapsed() < Duration::from_secs(3), "took {:?}", start.elapsed());
    assert!(stdout.contains("[slow] cancelled"), "{stdout}");
    assert!(stdout.contains("❌ [bad] failed"), "{stdout}");
    assert!(stdout.contains("Command: sleep 0.1; printf failure-out; printf failure-err >&2; exit 5"), "{stdout}");
    assert!(stdout.contains("Exit code: 5"), "{stdout}");
    assert!(stdout.contains("--- stdout ---\nfailure-out"), "{stdout}");
    assert!(stdout.contains("--- stderr ---\nfailure-err"), "{stdout}");
    assert!(!stdout.contains("should-not-appear"), "{stdout}");
}

#[test]
fn verbose_prints_prefixed_output_from_both_streams() {
    let output = run(&["--no-timer", "--verbose"], "echo hello; echo problem >&2 # example\n");
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
