import { Component, type ReactNode } from 'react'
import { Alert, Button, Code, Stack, Text } from '@mantine/core'

interface Props {
  children: ReactNode
}
interface State {
  error: Error | null
}

/**
 * Top-level safety net: a render/effect error anywhere below shows a recoverable
 * message instead of unmounting the whole app to a blank screen.
 */
export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: { componentStack?: string }) {
    // Surface the error for diagnosis (visible in the browser console).
    console.error('Workspace crashed:', error, info)
  }

  render() {
    const { error } = this.state
    if (!error) return this.props.children
    return (
      <div style={{ padding: 24, height: '100vh', overflow: 'auto' }}>
        <Alert color="red" variant="light" title="Something went wrong">
          <Stack gap="sm">
            <Text size="sm">
              The workspace hit an unexpected error. Your work is saved locally — reloading
              usually recovers it.
            </Text>
            <Code block>{error.message}</Code>
            <div>
              <Button size="xs" onClick={() => this.setState({ error: null })} mr="xs">
                Try to recover
              </Button>
              <Button size="xs" variant="default" onClick={() => globalThis.location.reload()}>
                Reload
              </Button>
            </div>
          </Stack>
        </Alert>
      </div>
    )
  }
}
