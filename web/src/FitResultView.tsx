// FitResultView.tsx — the shared curve-fit result rendering (error alert, or
// the convergence badge + parameter table), used by CurveFitModal and the
// Digitizer's fit dialog. Kept component-only (react-refresh); the shared
// constants and pure helpers live in curveFitShared.ts.

import { Alert, Badge, Group, Stack, Table, Text, TextInput } from '@mantine/core'
import { CurveFitResponse } from './api'
import { formatValue } from './format'
import { MONO_INPUT } from './curveFitShared'

/** Fit outcome rendering: error alert, or the convergence badge + parameter
 * table. The unit column renders only when `setParameterUnits` is provided
 * (the digitizer's data is unitless — it omits the editor). */
export function FitResultView({
  result,
  parameterUnits,
  setParameterUnits,
}: Readonly<{
  result: CurveFitResponse
  parameterUnits?: Record<string, string>
  setParameterUnits?: (units: Record<string, string>) => void
}>) {
  if (!result.success) {
    return (
      <Alert color="red" variant="light" p="xs">
        <Text size="sm" style={{ whiteSpace: 'pre-wrap' }}>
          {result.error}
        </Text>
      </Alert>
    )
  }
  const units = parameterUnits ?? {}
  return (
    <Stack gap="xs">
      <Group gap="xs">
        <Badge color="green" variant="light" leftSection="✓">
          Fit converged
        </Badge>
        <Text size="xs" c="dimmed">
          {result.iterations} iterations · R² = {formatValue(result.rSquared)} · RMSE ={' '}
          {formatValue(result.rmse)}
        </Text>
      </Group>
      <Table striped highlightOnHover>
        <Table.Thead>
          <Table.Tr>
            <Table.Th style={{ width: '120px' }}>Parameter</Table.Th>
            <Table.Th style={{ width: '180px' }}>Fitted value</Table.Th>
            {setParameterUnits && <Table.Th>Unit (optional)</Table.Th>}
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {result.parameterNames.map((name, i) => (
            <Table.Tr key={name}>
              <Table.Td ff="monospace">{name}</Table.Td>
              <Table.Td ff="monospace" c="green.4">
                {formatValue(result.fittedParameters[i])}
              </Table.Td>
              {setParameterUnits && (
                <Table.Td>
                  <TextInput
                    size="xs"
                    placeholder="e.g. kPa"
                    value={units[name] || ''}
                    onChange={(e) => {
                      setParameterUnits({
                        ...units,
                        [name]: e.currentTarget.value,
                      })
                    }}
                    styles={MONO_INPUT}
                  />
                </Table.Td>
              )}
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Stack>
  )
}
