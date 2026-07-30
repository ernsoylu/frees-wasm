#!/usr/bin/env bash
# Classpath for the reference frees engine.
#
# The golden dumper already solved this problem — the ANTLR version pin, the
# SLF4J provider conflict, picking the newest core jar — and getting it wrong
# fails in ways that look like engine bugs. There is exactly one copy of that
# logic; this is a delegation, not a second implementation.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$here/../golden-dumper/classpath.sh" "$@"
