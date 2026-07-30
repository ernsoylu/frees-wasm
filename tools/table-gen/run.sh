#!/usr/bin/env bash
# Compile and run the CoolProp property-table generator.
#
#   ./run.sh [out-dir] [options...]
#
# Defaults to fixtures/proptables, relative to the repo root. Options are passed
# straight through to TableGen (run with no arguments for the list).
#
# Examples:
#   ./run.sh                                   # Water + R134a at the default grid
#   ./run.sh /tmp/tables --sweep               # resolution ladder, writes no tables
#   ./run.sh /tmp/tables --fluids R1234yf      # one extra fluid
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
build="$here/build"

if [[ $# -gt 0 && "$1" != --* ]]; then
  out="$1"
  shift
else
  out="$root/fixtures/proptables"
fi

cp="$("$here/classpath.sh")"
mkdir -p "$build"
javac -nowarn -cp "$cp" -d "$build" "$here/TableGen.java"

# Every value this tool writes comes from the native library; without it there
# is nothing to generate. Same lookup as tools/golden-dumper/run.sh.
if [[ -z "${COOLPROP_LIBRARY:-}" ]]; then
  candidate="${FREES_HOME:-/home/eren/dev/frEES}/backend/core/native/libCoolProp.so"
  if [[ -f "$candidate" ]]; then
    export COOLPROP_LIBRARY="$candidate"
    echo "CoolProp: $candidate" >&2
  else
    echo "error: no libCoolProp.so found — set COOLPROP_LIBRARY or FREES_HOME" >&2
    exit 1
  fi
fi

exec java --enable-native-access=ALL-UNNAMED \
  -cp "$build:$cp" com.frees.backend.props.TableGen "$out" "$@"
