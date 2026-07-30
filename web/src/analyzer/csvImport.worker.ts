// CSV import Web Worker — thin shell around the pure CsvIngest (csvImport.ts).
//
// Streams the File through papaparse in chunks (papaparse uses FileReader, so
// the whole file is never materialized as one string) and hands the finished
// Float64Array buffers back to the main thread as Transferable Objects.
//
// ORDERING RULE (todo.md §2): transferring detaches the buffers in this worker
// instantly, so every derived result (per-column min/max, monotonicity
// validation, kind sniffing, the header hash) is computed BEFORE the single
// postMessage below — reading a detached buffer throws. All of those are
// accumulated streamingly inside CsvIngest or assembled into the message
// object itself, so nothing touches the buffers after transfer.

import Papa from 'papaparse'
import { CsvImportError, CsvIngest, headerHash, type WorkerRequest, type WorkerResponse } from './csvImport'

// tsconfig lib is DOM (self: Window), so route postMessage through the worker
// signature explicitly.
const post = (msg: WorkerResponse, transfer?: Transferable[]) =>
  (self as unknown as { postMessage: (m: WorkerResponse, t?: Transferable[]) => void }).postMessage(
    msg,
    transfer ?? [],
  )

function fail(err: unknown) {
  if (err instanceof CsvImportError) {
    post({ type: 'error', message: err.message, code: err.code, rows: err.rows })
  } else {
    post({ type: 'error', message: err instanceof Error ? err.message : String(err) })
  }
}

self.addEventListener('message', (event) => {
  // A dedicated worker only receives messages from the page that spawned it
  // (origin is "" for those); reject anything else defensively.
  if (event.origin !== '' && event.origin !== globalThis.location.origin) return
  const { file, choice } = (event as MessageEvent<WorkerRequest>).data
  const ingest = new CsvIngest(choice)
  let failed = false

  const head64k = file.slice(0, 65536).text()

  Papa.parse<string[]>(file, {
    skipEmptyLines: 'greedy',
    chunk(results, parser) {
      try {
        ingest.push(results.data)
        if (ingest.ask !== null) parser.abort()
      } catch (err) {
        failed = true
        parser.abort()
        fail(err)
      }
    },
    complete() {
      if (failed) return
      void head64k
        .then((head) => {
          const outcome = ingest.tryFinish()
          if (outcome.status === 'ask') {
            post({ type: 'needs-time', candidates: outcome.candidates })
            return
          }
          const { result } = outcome
          const transfer: Transferable[] = [result.time.buffer]
          for (const ch of result.channels) {
            if (ch.values !== null) transfer.push(ch.values.buffer)
          }
          post(
            {
              type: 'done',
              time: result.time,
              channels: result.channels,
              rowCount: result.rowCount,
              headerHash: headerHash(head, result.columnNames),
            },
            transfer,
          )
        })
        .catch(fail)
    },
    error(err) {
      if (!failed) {
        failed = true
        fail(err)
      }
    },
  })
})
