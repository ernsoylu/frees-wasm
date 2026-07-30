// Template-mode file relocation checks (design contract §2.5b).
//
// A `.frees` project stores measurement REFS only (measurementId + file
// signature); on load the samples are gone and the user re-picks the file.
// The re-picked file is verified against what the analyzer actually needs:
//  - channel-name match is MANDATORY — every channel referenced by a strip
//    must exist in the new file, otherwise the pick is rejected outright
//    (hard error, per the project's strict-over-warn policy);
//  - size / headerHash mismatch is ADVISORY — surfaced with an explicit
//    "use anyway" override (same recording re-exported, trimmed, etc.).

import type { FileSignature } from './types'

export type RelocationCheck =
  | { status: 'ok' }
  | { status: 'advisory'; mismatches: string[] }
  | { status: 'rejected'; missingChannels: string[] }

export function checkRelocatedFile(
  requiredChannels: readonly string[],
  newChannelNames: readonly string[],
  stored: FileSignature,
  incoming: { size: number; headerHash: string },
): RelocationCheck {
  const available = new Set(newChannelNames)
  const missing = requiredChannels.filter((c) => !available.has(c))
  if (missing.length > 0) return { status: 'rejected', missingChannels: missing }

  const mismatches: string[] = []
  if (incoming.size !== stored.size) mismatches.push('file size')
  if (incoming.headerHash !== stored.headerHash) mismatches.push('content hash')
  if (mismatches.length > 0) return { status: 'advisory', mismatches }
  return { status: 'ok' }
}
