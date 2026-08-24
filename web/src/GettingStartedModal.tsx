import { helpUrl } from './helpUrl'
import { Button, Group, Kbd, Modal, Paper, SimpleGrid, Stack, Text, ThemeIcon } from '@mantine/core'
import { IconBook2, IconLayoutGrid, IconPlayerPlayFilled, IconShieldCheck } from '@tabler/icons-react'

interface Props {
  opened: boolean
  onClose: () => void
  /** Solves whatever is in the editor — on a first visit, the shipped default example. */
  onSolveExample: () => void
  /** Opens the example gallery. */
  onOpenExamples: () => void
}

interface CardSpec {
  icon: React.ReactNode
  title: string
  body: string
  action: string
  onClick?: () => void
  href?: string
}

/**
 * The app's welcome mat: frees opens straight into the workspace (a deliberate
 * decision — no landing page), so this modal carries the orientation a landing
 * page would have: what frees is, and four one-click ways in. Auto-opens once
 * per browser on desktop; reopens from the command palette.
 */
export default function GettingStartedModal({ opened, onClose, onSolveExample, onOpenExamples }: Readonly<Props>) {
  const cards: CardSpec[] = [
    {
      icon: <IconPlayerPlayFilled size={18} />,
      title: 'Solve the loaded example',
      body: 'A complete EV thermal-management system is already in the editor — two coupled coolant loops, real fluids, sized heat exchangers. Watch it solve.',
      action: 'Solve it',
      onClick: () => {
        onClose()
        onSolveExample()
      },
    },
    {
      icon: <IconLayoutGrid size={18} />,
      title: 'Start from a worked example',
      body: 'Dozens of ready-to-solve models: power cycles, refrigeration, heat transfer, controls, dynamics, component networks.',
      action: 'Browse examples',
      onClick: () => {
        onClose()
        onOpenExamples()
      },
    },
    {
      icon: <IconBook2 size={18} />,
      title: 'Learn the basics',
      body: 'Seven short steps: your first solve, thinking declaratively, units and Check, tables and plots, the REPL, components.',
      action: 'Open the guide',
      href: helpUrl('#gs-first-solve'),
    },
    {
      icon: <IconShieldCheck size={18} />,
      title: 'Why trust the numbers',
      body: 'Every commit runs a validation suite of problems with independently derived answers — closed forms and public standards, not faith.',
      action: 'See the verification suite',
      href: helpUrl('#verification'),
    },
  ]

  return (
    <Modal opened={opened} onClose={onClose} title="Welcome to frees" size="lg" centered>
      <Stack gap="md">
        <Text size="sm">
          frees is a <strong>declarative equation solver</strong> for engineers: write equations in
          any order, mark nothing as input or output — <Kbd>F4</Kbd> checks the system is
          well-posed, <Kbd>F2</Kbd> finds the unknowns. Units, real-fluid properties, uncertainty
          propagation and an acausal component library are built in.
        </Text>
        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm">
          {cards.map((card) => (
            <Paper key={card.title} withBorder p="sm" radius="md">
              <Stack gap={6} justify="space-between" h="100%">
                <Group gap="xs" wrap="nowrap">
                  <ThemeIcon variant="light" size="md" radius="md">
                    {card.icon}
                  </ThemeIcon>
                  <Text size="sm" fw={600}>
                    {card.title}
                  </Text>
                </Group>
                <Text size="xs" c="dimmed">
                  {card.body}
                </Text>
                {card.href ? (
                  <Button
                    component="a"
                    href={card.href}
                    target="_blank"
                    rel="noopener"
                    size="xs"
                    variant="light"
                  >
                    {card.action}
                  </Button>
                ) : (
                  <Button size="xs" variant="light" onClick={card.onClick}>
                    {card.action}
                  </Button>
                )}
              </Stack>
            </Paper>
          ))}
        </SimpleGrid>
        <Group justify="space-between" wrap="wrap" gap="xs">
          <Text size="xs" c="dimmed">
            <Kbd>F4</Kbd> Check · <Kbd>F2</Kbd> Solve · <Kbd>Ctrl</Kbd>+<Kbd>K</Kbd> commands ·{' '}
            <Kbd>F1</Kbd> help on the symbol under the cursor
          </Text>
          <Text size="xs" c="dimmed">
            Reopen anytime: <Kbd>Ctrl</Kbd>+<Kbd>K</Kbd> → “Getting Started”
          </Text>
        </Group>
      </Stack>
    </Modal>
  )
}
