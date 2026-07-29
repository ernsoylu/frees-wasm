#!/usr/bin/env bash
# Compile and run the golden dumper over the fixture corpus.
#
#   ./run.sh [corpus-dir] [output-dir]
#
# Defaults to fixtures/corpus -> fixtures/golden, both relative to the repo root.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
corpus="${1:-$root/fixtures/corpus}"
out="${2:-$root/fixtures/golden}"
build="$here/build"

cp="$("$here/classpath.sh")"
mkdir -p "$build"
javac -nowarn -cp "$cp" -d "$build" "$here/GoldenDumper.java"
exec java -cp "$build:$cp" GoldenDumper "$corpus" "$out"
