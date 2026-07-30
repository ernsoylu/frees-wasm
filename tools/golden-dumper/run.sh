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

# CoolProp is loaded by JNA from an explicit path (core/build.gradle sets the
# same variable for its own test task). Without it every real-fluid property
# call fails with "The CoolProp native library is not available" and the
# resulting goldens record that failure instead of a value — which silently
# turns fluid documents into useless fixtures.
if [[ -z "${COOLPROP_LIBRARY:-}" ]]; then
  candidate="${FREES_HOME:-/home/eren/dev/frEES}/backend/core/native/libCoolProp.so"
  if [[ -f "$candidate" ]]; then
    export COOLPROP_LIBRARY="$candidate"
    echo "CoolProp: $candidate" >&2
  else
    echo "warning: no libCoolProp.so found — fluid-property goldens will record a load failure" >&2
  fi
fi

exec java -cp "$build:$cp" GoldenDumper "$corpus" "$out"
