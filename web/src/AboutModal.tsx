import { useEffect, useState } from 'react'
import { Anchor, Divider, Group, Modal, Stack, Text } from '@mantine/core'
import { IconBrandGithub, IconBrandLinkedin } from '@tabler/icons-react'
import { COMMIT_HASH, COMMIT_IS_REAL, COMMIT_SHORT, VERSION_LABEL } from './version'
import { wasmVersion } from './wasm/engineClient'

interface Props {
  onClose: () => void
}

const REPO_URL = 'https://github.com/ernsoylu/frees'

// About card: software identity + license on top, author at the bottom.
export default function AboutModal({ onClose }: Readonly<Props>) {
  // The in-browser engine's own version, from the wasm worker handshake.
  // null until it answers; a dead worker leaves the row out rather than
  // blocking the dialog.
  const [engineVersion, setEngineVersion] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    wasmVersion()
      .then((v) => {
        if (!cancelled) setEngineVersion(v)
      })
      .catch(() => {
        /* worker unavailable — omit the row */
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <Modal opened onClose={onClose} title="About" centered size="sm">
      <Stack gap="sm">
        <Stack gap={2}>
          <Group gap="xs" align="baseline">
            <Text fw={700} size="xl">
              frees
            </Text>
            <Text c="dimmed" size="sm">
              free solver · {VERSION_LABEL}
            </Text>
          </Group>
          <Text size="sm">
            A free, web-based, open-source equation-solving environment for
            engineers. Solves systems of non-linear simultaneous equations with
            symbolic compiling, unit handling, and uncertainty propagation.
          </Text>
          <Group gap="xs" wrap="nowrap" mt={4}>
            <IconBrandGithub size={20} stroke={1.6} />
            <Anchor href={REPO_URL} target="_blank" rel="noopener noreferrer" size="sm">
              github.com/ernsoylu/frees
            </Anchor>
          </Group>
          <Text c="dimmed" size="xs">
            Build:{' '}
            {COMMIT_IS_REAL ? (
              <Anchor
                href={`${REPO_URL}/commit/${COMMIT_HASH}`}
                target="_blank"
                rel="noopener noreferrer"
                inherit
                ff="monospace"
              >
                {COMMIT_SHORT}
              </Anchor>
            ) : (
              <Text span inherit ff="monospace">
                dev (local build)
              </Text>
            )}
          </Text>
          {engineVersion && (
            <Text c="dimmed" size="xs">
              Engine:{' '}
              <Text span inherit ff="monospace">
                frees-core {engineVersion} (wasm)
              </Text>
            </Text>
          )}
        </Stack>

        <Text c="dimmed" size="xs">
          Licensed under the MIT License. Copyright © 2026 Eren Soylu. Provided
          “as is”, without warranty of any kind.
        </Text>

        <Divider />

        <Stack gap={6}>
          <Text fw={600} size="sm">
            Author
          </Text>
          <Text size="sm">Eren Soylu</Text>
          <Group gap="xs" wrap="nowrap">
            <IconBrandLinkedin size={20} stroke={1.6} />
            <Anchor
              href="https://uk.linkedin.com/in/erensoylu"
              target="_blank"
              rel="noopener noreferrer"
              size="sm"
            >
              linkedin.com/in/erensoylu
            </Anchor>
          </Group>
          <Group gap="xs" wrap="nowrap">
            <IconBrandGithub size={20} stroke={1.6} />
            <Anchor
              href="https://github.com/ernsoylu"
              target="_blank"
              rel="noopener noreferrer"
              size="sm"
            >
              github.com/ernsoylu
            </Anchor>
          </Group>
        </Stack>
      </Stack>
    </Modal>
  )
}
