/** CSV download for the Tables workbook: quote-double `"` and wrap any cell
 *  containing a comma, quote, or newline. */
export function downloadValuesAsCsv(values: unknown[][], filename: string): void {
  const csvStr = values
    .map((row) =>
      row
        .map((cell) => {
          let val = String(cell ?? '').replaceAll('"', '""')
          if (val.includes(',') || val.includes('"') || val.includes('\n')) val = `"${val}"`
          return val
        })
        .join(','),
    )
    .join('\n')
  const blob = new Blob([csvStr], { type: 'text/csv' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  URL.revokeObjectURL(url)
}
