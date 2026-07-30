import { compressToEncodedURIComponent, decompressFromEncodedURIComponent } from 'lz-string'

/**
 * Share-by-URL: the whole document travels in the URL fragment as
 * `#share=<lz-string>`, so a link is self-contained — no server storage, no
 * account, nothing to expire. The fragment never leaves the browser (it is
 * not sent in the HTTP request), so sharing stays client-side end to end.
 */

const SHARE_PREFIX = '#share='

/** Ceiling for the emitted URL. Modern browsers handle far longer URLs, but
 *  chat apps, terminals and older proxies start mangling somewhere in the tens
 *  of thousands of characters — past this, refuse rather than emit a link
 *  that truncates silently. */
export const MAX_SHARE_URL_CHARS = 16000

/** Builds a share URL for the given document text, or null when the
 *  compressed link would exceed {@link MAX_SHARE_URL_CHARS}. */
export function buildShareUrl(text: string, base?: string): string | null {
  const origin = base ?? globalThis.location.origin + globalThis.location.pathname
  const url = origin + SHARE_PREFIX + compressToEncodedURIComponent(text)
  return url.length > MAX_SHARE_URL_CHARS ? null : url
}

/** Extracts the document text carried by a `#share=` fragment, or null when
 *  the hash is absent, not a share link, or fails to decompress (a truncated
 *  or hand-mangled link must open the normal workspace, not throw). */
export function extractSharedText(hash: string): string | null {
  if (!hash.startsWith(SHARE_PREFIX)) {
    return null
  }
  const payload = hash.slice(SHARE_PREFIX.length)
  if (payload.length === 0) {
    return null
  }
  try {
    const text = decompressFromEncodedURIComponent(payload)
    return text === null || text.length === 0 ? null : text
  } catch {
    return null
  }
}

/** Removes the share fragment from the address bar without reloading, so a
 *  refresh returns to the user's own autosaved work instead of re-importing
 *  the shared document. */
export function clearShareHash(): void {
  if (globalThis.location.hash.startsWith(SHARE_PREFIX)) {
    globalThis.history.replaceState(null, '', globalThis.location.pathname + globalThis.location.search)
  }
}
