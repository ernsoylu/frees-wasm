#!/usr/bin/env bash
# Assemble a classpath for the reference frees engine.
#
# Prints the classpath on stdout; diagnostics go to stderr so this can be used
# as `CP=$(./classpath.sh)`.
#
# Two things make this non-trivial, both learned the hard way:
#
#   1. The Gradle cache holds MULTIPLE versions of some artifacts. ANTLR is the
#      dangerous one — an older `antlr4-runtime` on the classpath ahead of the
#      pinned 4.13.2 makes the generated parser fail at class-initialisation
#      with a version-mismatch error. Pinned jars therefore go FIRST and the
#      known-bad versions are filtered out entirely.
#   2. Several SLF4J providers are present. Logback is excluded so the binding
#      is unambiguous and the engine's own logging stays quiet.
#
# This reads the reference repo and the Gradle cache. It never writes to either.

set -euo pipefail

FREES_HOME="${FREES_HOME:-/home/eren/dev/frEES}"
GRADLE_CACHE="${GRADLE_CACHE:-$HOME/.gradle/caches/modules-2/files-2.1}"

if [[ ! -d "$FREES_HOME" ]]; then
  echo "error: reference repo not found at $FREES_HOME (set FREES_HOME)" >&2
  exit 1
fi

# Newest built core jar. Prefer a clean (non "-dirty") build when one exists at
# the same mtime rank, since a dirty jar may not correspond to any commit.
core_jar="${FREES_CORE_JAR:-}"
if [[ -z "$core_jar" ]]; then
  core_jar=$(ls -t "$FREES_HOME"/backend/core/build/libs/core-*.jar 2>/dev/null | head -1 || true)
fi
if [[ -z "$core_jar" || ! -f "$core_jar" ]]; then
  echo "error: no core jar under $FREES_HOME/backend/core/build/libs/" >&2
  echo "       build one with: (cd $FREES_HOME/backend && ./gradlew :core:jar)" >&2
  exit 1
fi
echo "core jar: $(basename "$core_jar")" >&2

if [[ ! -d "$GRADLE_CACHE" ]]; then
  echo "error: gradle cache not found at $GRADLE_CACHE" >&2
  exit 1
fi

# Versions pinned by ../frEES/backend/core/build.gradle that must win.
pinned=()
antlr=$(find "$GRADLE_CACHE/org.antlr/antlr4-runtime/4.13.2" -name '*.jar' 2>/dev/null | head -1 || true)
if [[ -n "$antlr" ]]; then
  pinned+=("$antlr")
else
  echo "warning: antlr4-runtime 4.13.2 not in the cache; parsing will likely fail" >&2
fi

# Everything else, minus sources/javadoc and the conflicting artifacts.
mapfile -t rest < <(
  find "$GRADLE_CACHE" -name '*.jar' 2>/dev/null \
    | grep -v -e '-sources' -e '-javadoc' \
              -e 'antlr4-runtime/4\.7' \
              -e 'logback-classic' -e 'logback-core'
)

printf '%s' "$core_jar"
for jar in "${pinned[@]}" "${rest[@]}"; do
  printf ':%s' "$jar"
done
printf '\n'
