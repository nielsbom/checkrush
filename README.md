# checkrush

A Rust CLI command that runs tasks in parallel. On the first failure, it stops
the other commands and reports the failed command and its output.

`checkrush` is useful when you or an agent repeatedly have to run the same
independent tasks that usually succeed.

`checkrush` will:

- run all given tasks in parallel to minimize waiting time
- stop remaining tasks on the first failure and show a report of the failure

Alternatives:

- [rush (Go)](https://github.com/shenwei356/rush)
- [parallel (Ruby)](https://github.com/grosser/parallel)

## AI, LLM, vibecoding, slop

This command has completely been vibecoded. I don't know Rust :-). FYI.

## Quick start

- `cargo install checkrush` to install
- `printf 'npm run lint # lint\n npm run typecheck # typecheck\n npm run test:ci # tests' | checkrush`

## Installation

Requires the Rust toolchain and a Unix-like system. The current version has been
tested on macOS; Linux support has not yet been verified. Windows is not supported.

Install with:

```sh
cargo install checkrush
```

Alternatively, install the latest version from GitHub:

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

## Usage

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

### Larger examples

`checkrush` is useful for running independent project checks, such as
typechecking, linting, and tests. Long-running checks can also benefit from
parallel execution, provided they can be stopped safely on failure.

Here are examples of running checks in a Python project and a
TypeScript/JavaScript project. Run these from the project root and adjust the
commands to match the project. `--no-timer` keeps output readable in CI and
agent logs.

### Python

```sh
printf '%s\n' \
  'ruff check . # lint' \
  'mypy . # typecheck' \
  'pytest # tests' |
  checkrush --no-timer
```

(This obviously requires your project to use the tools in the example)

### TypeScript/JavaScript

```sh
printf '%s\n' \
  'npm run lint # lint' \
  'npm run typecheck # typecheck' \
  'npm run test:ci # tests' |
  checkrush --no-timer
```

(This obviously requires your project to use the tools in the example)

You can also put these commands in scripts so you can run them more easily:
[backend-checks.sh](examples/backend-checks.sh) for Python and
[frontend-checks.sh](examples/frontend-checks.sh) for TypeScript/JavaScript.
Copy the appropriate script into your project and run it from the project root:

```sh
./backend-checks.sh
# or
./frontend-checks.sh
```

Run any required generation or build steps before starting checkrush. Only run
checks in parallel if they are independent and do not modify the same files.

## Instructions for project agents

Copy the relevant snippet into the project's `AGENTS.md` and adapt the commands
and prerequisites to that project. These are examples, not requirements for
projects that use checkrush.

### Python AGENTS.md snippet

````markdown
## Verification

Before finishing changes, run these checks from the project root using the
project's Python environment. Ruff, mypy, pytest, and checkrush must be available.

```sh
printf '%s\n' \
  'ruff check . # lint' \
  'mypy . # typecheck' \
  'pytest # tests' |
  checkrush --no-timer
```

Checkrush stops remaining checks on the first failure. Fix failures and rerun;
do not report verification as complete until all checks pass.
````

### TypeScript/JavaScript AGENTS.md snippet

````markdown
## Verification

Before finishing changes, run these checks from the project root. Ensure the
project's dependencies, checkrush, and the `lint`, `typecheck`, and `test:ci` npm
scripts are available. `test:ci` must run once, without watch mode.

```sh
printf '%s\n' \
  'npm run lint # lint' \
  'npm run typecheck # typecheck' \
  'npm run test:ci # tests' |
  checkrush --no-timer
```

Checkrush stops remaining checks on the first failure. Fix failures and rerun;
do not report verification as complete until all checks pass.
````

## Test

```sh
cargo test
```

## License

Licensed under the [GNU Affero General Public License, version 3 or later](LICENSE).
