#!/usr/bin/env bash
# Prints the path of the reference frees repo (the read-only oracle).
#
# `$FREES_HOME` wins when set. Otherwise the reference is a SIBLING of this
# repo, as CLAUDE.md describes it — both spellings the directory has worn are
# tried, newest-first, so a checkout that has one or the other just works.
#
# Sourced by the oracle tools instead of each one hard-coding an absolute path
# that is only correct on the machine it was written on.
set -euo pipefail

_fh_here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
_fh_root="$(cd "$_fh_here/.." && pwd)"

if [[ -n "${FREES_HOME:-}" ]]; then
  echo "$FREES_HOME"
  exit 0
fi
for _fh_candidate in "$_fh_root/../frees" "$_fh_root/../frEES"; do
  if [[ -d "$_fh_candidate" ]]; then
    (cd "$_fh_candidate" && pwd)
    exit 0
  fi
done
echo "$_fh_root/../frees"
