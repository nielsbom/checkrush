use std::io::{self, BufRead, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(100);
const KILL_DELAY: Duration = Duration::from_millis(500);

struct Options {
    verbose: bool,
    no_timer: bool,
}

struct CommandSpec {
    name: String,
    command: String,
}

struct CommandResult {
    index: usize,
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

enum Event {
    Exited { index: usize, status: ExitStatus },
    Output(CommandResult),
    WaitError(io::Error),
}

struct Child {
    pid: i32,
    started: Instant,
    result: Option<CommandResult>,
    reported: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options {
        verbose: false,
        no_timer: false,
    };
    let mut seen = std::collections::HashSet::new();
    for arg in std::env::args().skip(1) {
        if !seen.insert(arg.clone()) {
            return Err(format!("Duplicate argument: {arg}"));
        }
        match arg.as_str() {
            "--verbose" | "-v" => options.verbose = true,
            "--no-timer" => options.no_timer = true,
            "--help" | "-h" => {
                println!("Usage: checkrush [--verbose|-v] [--no-timer] < commands.txt");
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
    }
    Ok(options)
}

fn parse_line(line: &str) -> Option<CommandSpec> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (command, name) = line.split_once(" #").unwrap_or((line, ""));
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let name = name.trim();
    Some(CommandSpec {
        name: if name.is_empty() { command } else { name }.to_owned(),
        command: command.to_owned(),
    })
}

fn seconds(start: Instant) -> String {
    format!("{:.1}", start.elapsed().as_secs_f64())
}

fn draw_timer(start: Instant) {
    print!("\relapsed: {}s\x1b[K", seconds(start));
    let _ = io::stdout().flush();
}

fn status_line(text: &str, timer: bool, start: Instant) {
    if timer {
        print!("\r\x1b[K");
    }
    print!("{text}");
    if timer {
        draw_timer(start);
    } else {
        let _ = io::stdout().flush();
    }
}

fn output_lines(name: &str, text: &str, tag: &str) {
    for line in text.lines() {
        println!("[{name}]{tag} {line}");
    }
}

fn print_output(spec: &CommandSpec, result: &CommandResult) {
    output_lines(&spec.name, &result.stdout, "");
    output_lines(&spec.name, &result.stderr, " E");
}

fn read_output(mut stream: impl Read) -> String {
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn start_command(
    index: usize,
    spec: &CommandSpec,
    tx: mpsc::Sender<Event>,
) -> io::Result<Child> {
    let mut process = Command::new("sh")
        .args(["-c", &spec.command])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let pid = process.id() as i32;
    let stdout = process.stdout.take().expect("stdout was piped");
    let stderr = process.stderr.take().expect("stderr was piped");
    let started = Instant::now();

    thread::spawn(move || {
        let stdout = thread::spawn(move || read_output(stdout));
        let stderr = thread::spawn(move || read_output(stderr));
        match process.wait() {
            Ok(status) => {
                let _ = tx.send(Event::Exited { index, status });
                let stdout = stdout.join().unwrap_or_default();
                let stderr = stderr.join().unwrap_or_default();
                let _ = tx.send(Event::Output(CommandResult {
                    index,
                    status,
                    stdout,
                    stderr,
                }));
            }
            Err(error) => {
                let _ = tx.send(Event::WaitError(error));
            }
        }
    });

    Ok(Child {
        pid,
        started,
        result: None,
        reported: false,
    })
}

fn signal_running(children: &[Child], signal: i32) {
    for child in children.iter().filter(|child| child.result.is_none()) {
        // SAFETY: Each PID identifies a process group started by this runner.
        unsafe {
            libc::kill(-child.pid, signal);
        }
    }
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

fn print_failure(spec: &CommandSpec, result: &CommandResult, verbose: bool) {
    println!("❌ [{}] failed", spec.name);
    println!("Command: {}", spec.command);
    match result.status.signal() {
        Some(signal) => println!("Exit code: {} (signal {signal})", exit_code(result.status)),
        None => println!("Exit code: {}", exit_code(result.status)),
    }
    if verbose {
        print_output(spec, result);
    } else {
        for (label, text) in [("stdout", &result.stdout), ("stderr", &result.stderr)] {
            if !text.is_empty() {
                print!("\n--- {label} ---\n{text}");
                if !text.ends_with('\n') {
                    println!();
                }
            }
        }
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let start = Instant::now();
    let options = match parse_args() {
        Ok(options) => options,
        Err(error) => {
            println!("{error}\nUsage: checkrush [--verbose|-v] [--no-timer] < commands.txt");
            return Ok(2);
        }
    };
    let lines = io::stdin().lock().lines().collect::<Result<Vec<_>, _>>()?;
    let specs: Vec<_> = lines.iter().filter_map(|line| parse_line(line)).collect();
    if specs.is_empty() {
        println!("no commands provided");
        return Ok(0);
    }

    let interrupted = Arc::new(AtomicBool::new(false));
    let terminated = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))?;
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&terminated))?;

    let timer = !options.no_timer;
    if timer {
        draw_timer(start);
    }
    let (tx, rx) = mpsc::channel();
    let mut children = Vec::with_capacity(specs.len());
    for (index, spec) in specs.iter().enumerate() {
        status_line(&format!("🚀 [{}] started\n", spec.name), timer, start);
        match start_command(index, spec, tx.clone()) {
            Ok(child) => children.push(child),
            Err(error) => {
                signal_running(&children, libc::SIGKILL);
                return Err(error.into());
            }
        }
    }
    drop(tx);

    let mut remaining = children.len();
    let mut failure = None;
    let mut failed_elapsed = String::new();
    let mut stopping = None;
    let mut forced = false;
    let mut interrupted_code = None;

    while remaining > 0 {
        if stopping.is_none() {
            let signal = if interrupted.load(Ordering::Relaxed) {
                Some(libc::SIGINT)
            } else if terminated.load(Ordering::Relaxed) {
                Some(libc::SIGTERM)
            } else {
                None
            };
            if let Some(signal) = signal {
                interrupted_code = Some(128 + signal);
                signal_running(&children, libc::SIGTERM);
                stopping = Some(Instant::now());
            }
        }
        if let Some(stopped_at) = stopping {
            if !forced && stopped_at.elapsed() >= KILL_DELAY {
                signal_running(&children, libc::SIGKILL);
                forced = true;
            }
        }

        match rx.recv_timeout(TICK) {
            Ok(Event::Exited { index, status }) => {
                if stopping.is_none() && !status.success() {
                    failure = Some(index);
                    failed_elapsed = seconds(start);
                    signal_running(&children, libc::SIGTERM);
                    stopping = Some(Instant::now());
                }
            }
            Ok(Event::Output(result)) => {
                let index = result.index;
                remaining -= 1;
                children[index].result = Some(result);
                let result = children[index].result.as_ref().unwrap();
                if stopping.is_none() {
                    let mut line = format!(
                        "✅ [{}] succeeded in {}s (elapsed {}s)\n",
                        specs[index].name,
                        seconds(children[index].started),
                        seconds(start)
                    );
                    if options.verbose {
                        for (text, tag) in [(&result.stdout, ""), (&result.stderr, " E")] {
                            for output in text.lines() {
                                line.push_str(&format!("[{}]{tag} {output}\n", specs[index].name));
                            }
                        }
                    }
                    status_line(&line, timer, start);
                    children[index].reported = true;
                }
            }
            Ok(Event::WaitError(error)) => {
                signal_running(&children, libc::SIGKILL);
                return Err(error.into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if timer && stopping.is_none() {
                    draw_timer(start);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("failed to collect command results".into());
            }
        }
    }

    if timer {
        print!("\r\x1b[K");
    }
    if let Some(code) = interrupted_code {
        return Ok(code);
    }
    if let Some(failing_index) = failure {
        for (index, child) in children.iter().enumerate() {
            if index == failing_index {
                continue;
            }
            let result = child.result.as_ref().unwrap();
            if result.status.success() && !child.reported {
                println!(
                    "✅ [{}] succeeded in {}s (elapsed {}s)",
                    specs[index].name,
                    seconds(child.started),
                    seconds(start)
                );
                if options.verbose {
                    print_output(&specs[index], result);
                }
            } else if matches!(result.status.signal(), Some(libc::SIGTERM | libc::SIGKILL)) {
                println!("⚠️  [{}] cancelled after {}s", specs[index].name, seconds(child.started));
            }
        }
        let result = children[failing_index].result.as_ref().unwrap();
        print_failure(&specs[failing_index], result, options.verbose);
        println!("failed after {failed_elapsed}s");
        Ok(exit_code(result.status))
    } else {
        println!("all passed in {}s", seconds(start));
        Ok(0)
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
