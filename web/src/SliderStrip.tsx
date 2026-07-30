import { ActionIcon, Group, Paper, Slider, Stack, Text, Tooltip } from '@mantine/core'
import { IconX } from '@tabler/icons-react'
import { formatValue } from './format'
import { sliderStep, type PinnedSlider } from './sliders'

interface Props {
  pins: PinnedSlider[]
  /** Dragging: update the displayed value (and, debounced, re-solve). */
  onChange: (name: string, value: number) => void
  /** Handle released: commit the value with a re-solve. */
  onCommit: (name: string, value: number) => void
  onUnpin: (name: string) => void
  /** True while a solve is in flight, so the strip can say so. */
  solving?: boolean
  /** Pins whose variable is no longer a literal parameter in the document —
   *  shown, but not applied, so an edit can never make a slider destructive. */
  inertNames?: string[]
}

/**
 * Pinned parameters as draggable sliders. Dragging rewrites the variable
 * through the same override path REPL assignments use, so the solution (and
 * every plot and table drawn from it) follows the handle.
 */
export default function SliderStrip({ pins, onChange, onCommit, onUnpin, solving, inertNames }: Readonly<Props>) {
  const inert = new Set((inertNames ?? []).map((n) => n.toLowerCase()))
  if (pins.length === 0) {
    return null
  }
  return (
    <Paper withBorder p="xs" radius="sm">
      <Group justify="space-between" mb={6}>
        <Text size="xs" fw={600} c="dimmed">
          Parameters
        </Text>
        {solving && (
          <Text size="xs" c="dimmed">
            solving…
          </Text>
        )}
      </Group>
      <Stack gap={10}>
        {pins.map((pin) => {
          const isInert = inert.has(pin.name.toLowerCase())
          return (
          <div key={pin.name}>
            <Group gap={6} wrap="nowrap" mb={2}>
              <Text size="xs" ff="monospace" style={{ flex: 1, minWidth: 0 }} truncate>
                {pin.name}
              </Text>
              <Text size="xs" ff="monospace" c={isInert ? 'dimmed' : 'teal'}>
                {formatValue(pin.value)}
                {pin.units && pin.units !== '-' ? ` ${pin.units}` : ''}
              </Text>
              <Tooltip label={`Unpin ${pin.name}`}>
                <ActionIcon
                  size="xs"
                  variant="subtle"
                  color="gray"
                  aria-label={`Unpin ${pin.name}`}
                  onClick={() => onUnpin(pin.name)}
                >
                  <IconX size={12} />
                </ActionIcon>
              </Tooltip>
            </Group>
            {isInert && (
              <Text size="xs" c="orange" mb={2}>
                no longer a parameter in the document — not applied
              </Text>
            )}
            <Slider
              size="sm"
              disabled={isInert}
              min={pin.min}
              max={pin.max}
              step={sliderStep(pin.min, pin.max)}
              value={pin.value}
              onChange={(v) => onChange(pin.name, v)}
              onChangeEnd={(v) => onCommit(pin.name, v)}
              label={(v) => formatValue(v)}
              thumbLabel={`${pin.name} value`}
            />
          </div>
          )
        })}
      </Stack>
    </Paper>
  )
}
