#!/bin/bash
set -Eeuo pipefail

printf '%s\n' \
    'npm run lint # lint' \
    'npm run typecheck # typecheck' \
    'npm run test:ci # tests' |
    checkrush --no-timer
