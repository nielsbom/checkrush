#!/bin/bash
set -Eeuo pipefail

printf '%s\n' \
    'ruff check . # lint' \
    'mypy . # typecheck' \
    'pytest # tests' |
    checkrush --no-timer
