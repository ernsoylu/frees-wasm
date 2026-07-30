#!/bin/sh
# Generate the real_ip trust list from TRUSTED_PROXY_CIDRS / TRUSTED_PROXY_HEADER.
#
# nginx can only recover the true client address if it knows which upstream hops
# are trustworthy and which header carries the client — both deployment
# specific, so neither is guessed here. Unset (the default) means no proxy is
# trusted: $remote_addr stays the actual connection peer and any forwarded
# header is ignored.
#
# Guessing is not safe. With a docker-published port the peer nginx sees is
# always the bridge gateway (172.x), so trusting the private ranges by default
# would make EVERY external client look like it came from a trusted proxy —
# handing anyone on the internet the ability to forge their own client address
# and hand themselves a fresh rate-limit bucket per request. That is the exact
# bypass this is meant to close, so the default has to be "trust nothing".
#
# Pick the CIDR from what the edge actually connects FROM, and the header from
# what that edge actually sets — measure both, do not assume:
#
#   Plain tunnel (Pangolin/Traefik), edge appends X-Forwarded-For:
#     TRUSTED_PROXY_CIDRS="10.0.0.0/8"
#
#   Cloudflare in front of Railway (this project's production):
#     TRUSTED_PROXY_CIDRS="100.64.0.0/10"      # Railway reaches the container
#                                              # over IPv4 CGNAT, NOT fd00::/8
#     TRUSTED_PROXY_HEADER="CF-Connecting-IP"
#   Here X-Forwarded-For is useless: Cloudflare replaces whatever the client
#   sent, and what arrives is "<cloudflare egress>, <railway edge>" — the real
#   client is absent and the last entry is one FIXED address shared by every
#   user, so trusting the chain would put the whole world in one rate-limit
#   bucket. CF-Connecting-IP carries the actual client and cannot be forged,
#   because Cloudflare overwrites it — but that holds ONLY while Cloudflare is
#   the sole ingress. If the service ever gains a second, non-Cloudflare domain,
#   anyone reaching it directly can forge the header, so drop this setting the
#   moment that stops being true.
set -e

target="/etc/nginx/real-ip.conf"
: > "$target"

[ -n "${TRUSTED_PROXY_CIDRS:-}" ] || exit 0

# Both values are interpolated into an nginx config, so both are validated
# rather than trusted: a malformed one would otherwise either fail `nginx -t`
# and leave the container in a restart loop, or smuggle in a directive.
for cidr in $(echo "$TRUSTED_PROXY_CIDRS" | tr ',' ' '); do
  [ -n "$cidr" ] || continue
  if printf '%s' "$cidr" | grep -qE '^[0-9a-fA-F.:]+(/[0-9]{1,3})?$'; then
    echo "set_real_ip_from $cidr;" >> "$target"
  else
    echo "10-real-ip.sh: ignoring TRUSTED_PROXY_CIDRS entry '$cidr' (not an address/CIDR)" >&2
  fi
done

# Nothing valid survived: leave the file empty rather than emitting a header
# directive with no trust list, which would be inert but misleading.
if [ ! -s "$target" ]; then
  echo "10-real-ip.sh: no valid entries in TRUSTED_PROXY_CIDRS; real_ip disabled" >&2
  : > "$target"
  exit 0
fi

header="${TRUSTED_PROXY_HEADER:-X-Forwarded-For}"
if ! printf '%s' "$header" | grep -qE '^[A-Za-z0-9-]+$'; then
  echo "10-real-ip.sh: TRUSTED_PROXY_HEADER '$header' is not a header name; real_ip disabled" >&2
  : > "$target"
  exit 0
fi
echo "real_ip_header $header;" >> "$target"

# real_ip_recursive walks a comma-separated chain right-to-left, skipping
# trusted hops. That is only meaningful for the append-only X-Forwarded-For;
# a single-value header like CF-Connecting-IP has no chain to walk.
if [ "$header" = "X-Forwarded-For" ]; then
  echo "real_ip_recursive on;" >> "$target"
fi
