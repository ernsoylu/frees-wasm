#!/bin/sh
# Bake the deploy's git commit into a static file the SPA reads, so the About
# dialog shows the exact running revision. Platforms expose the commit at
# RUNTIME (Railway sets RAILWAY_GIT_COMMIT_SHA), which build args can't capture — so
# we write it here, on container start, before nginx serves. Falls back to the
# build-time stamp baked by Vite when no runtime commit is present (e.g. local).
set -e
commit="${RAILWAY_GIT_COMMIT_SHA:-${BUILD_COMMIT:-}}"
target="/usr/share/nginx/html/build-info.js"

# The value is interpolated into a JavaScript file the SPA loads, so it is only
# ever emitted when it looks like a commit SHA. Anything else is dropped rather
# than quoted-and-hoped-for: a value containing a quote would close the string
# literal and everything after it would execute on every page load, in a script
# the page's CSP trusts as same-origin. Today this comes from the platform or
# from git, but "the input happens to be safe right now" is not what should be
# standing between an env var and script execution in every visitor's browser.
if [ -n "$commit" ]; then
  if printf '%s' "$commit" | grep -qE '^[0-9a-fA-F]{7,40}$'; then
    printf 'window.__BUILD_COMMIT__="%s";\n' "$commit" > "$target"
  else
    echo "40-build-info.sh: ignoring commit stamp '$commit' (not a hex SHA)" >&2
  fi
fi
