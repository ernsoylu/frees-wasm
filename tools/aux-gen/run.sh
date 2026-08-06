#!/usr/bin/env bash
# Compile and run the CoolProp auxiliary-grid generator.
#
#   ./run.sh [out-dir] [options...]
#
# Defaults to fixtures/auxtables, relative to the repo root. Options are passed
# straight through to AuxGen.
#
# Examples:
#   ./run.sh                                   # every grid, default resolution
#   ./run.sh /tmp/aux --only MEG               # one family
#   ./run.sh /tmp/aux --ntau 64                # a finer incompressible grid
#     (the concentration axis is fixed at 1 % steps by design — see README.md)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
build="$here/build"

if [[ $# -gt 0 && "$1" != --* ]]; then
  out="$1"
  shift
else
  out="$root/fixtures/auxtables"
fi

cp="$("$here/../golden-dumper/classpath.sh")"
mkdir -p "$build"
# GenSupport carries the byte sink, the SHA-256, the JSON writer and the
# CoolProp-version binding that both generators need; there is no jar, so it
# is compiled into this tool's own build directory alongside it.
javac -nowarn -cp "$cp" -d "$build" "$here/../shared/GenSupport.java" "$here/AuxGen.java"

# Every value this tool writes comes from the native library; without it there
# is nothing to generate. Same lookup as tools/table-gen/run.sh.
if [[ -z "${COOLPROP_LIBRARY:-}" ]]; then
  candidate="$("$here/../frees-home.sh")/backend/core/native/libCoolProp.so"
  if [[ -f "$candidate" ]]; then
    export COOLPROP_LIBRARY="$candidate"
    echo "CoolProp: $candidate" >&2
  else
    echo "error: no libCoolProp.so found — set COOLPROP_LIBRARY or FREES_HOME" >&2
    exit 1
  fi
fi

exec java --enable-native-access=ALL-UNNAMED \
  -cp "$build:$cp" com.frees.backend.props.AuxGen "$out" "$@"
