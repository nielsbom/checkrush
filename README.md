# checkrush

A vibecoded Rust CLI that runs shell commands from stdin in parallel. On the
first failure, it stops the other commands and reports the failed command and
its output.

## Installation

Requires the Rust toolchain and a Unix-like system. The current version has been
tested on macOS; Linux support has not yet been verified. Windows is not supported.

Once published to [crates.io](https://crates.io/crates/checkrush), install with:

```sh
cargo install checkrush
```

Before the first crates.io release, install from GitHub instead:

```sh
cargo install --git https://github.com/nielsbom/checkrush
```

Cargo installs `checkrush` into `~/.cargo/bin` by default. Make sure that directory
is on your `PATH`.

To build from a local checkout without installing:

```sh
cargo build --release
```

The executable will be at `target/release/checkrush`.

## Use

Pipe one shell command per line:

```sh
printf 'echo hello # greet\nsleep 1 && echo done # wait\n' | checkrush
```

Or read commands from a file:

```sh
checkrush < commands.txt
```

An optional ` # name` suffix assigns a display name. Without one, the full
command is used as the name. Like `parallel-run`, the first ` #` in a line is
always treated as the name marker, even inside quotes. Each command runs with
`sh -c`, with stdin disconnected.

Options:

- `--verbose` or `-v`: print each command's captured output with a name prefix
  when it finishes. Standard error lines have an `E` prefix.
- `--no-timer`: disable the live elapsed timer, useful in CI or agent output.
- `--help` or `-h`: show usage.

Successful commands normally show only their names and timings. If a command
fails, its name, command, exit code, stdout, and stderr are reported. The exit
code is the failing command's exit code. If all commands succeed, checkrush
exits 0. Empty input also exits 0.

Commands run in separate Unix process groups. On failure, checkrush sends
SIGTERM to remaining groups, then SIGKILL after 500 ms if needed. Processes
that detach from their group, or processes running inside a container, may need
separate cleanup.

## Test

```sh
cargo test
```

## License

Licensed under the [GNU Affero General Public License, version 3 or later](LICENSE).
