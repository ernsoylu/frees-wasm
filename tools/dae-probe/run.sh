#!/usr/bin/env bash
# Compile and run the DAE ground-truth probe against the real SUNDIALS IDA.
#
#   ./run.sh [output.json]
#
# Defaults to fixtures/dae-oracle.json, relative to the repo root.
#
# The document-level oracle (tools/golden-dumper) cannot reach the IDA path
# without a DYNAMIC block that names an IDA method, so this drives the Java
# `IdaDaeSolver` / `DaeJacobian` / `SparseSteadyKlu` API directly. It is the
# ground truth `crates/frees-core/src/dae/solver.rs` is graded against.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/fixtures/dae-oracle.json}"
build="$here/build"

cp="$("$root/tools/golden-dumper/classpath.sh")"
mkdir -p "$build"
javac -nowarn -cp "$cp" -d "$build" "$here/DaeProbe.java"

# SUNDIALS >= 6 (the SUNContext API) is required. The Debian/Ubuntu package
# libsundials-dev installs libsundials_ida.so.6 in the default search path, so
# no explicit variable is normally needed; set SUNDIALS_LIBRARY to an explicit
# .so path to override.
if [[ -n "${SUNDIALS_LIBRARY:-}" ]]; then
  echo "SUNDIALS: $SUNDIALS_LIBRARY" >&2
fi

exec java -cp "$build:$cp" DaeProbe "$out"
