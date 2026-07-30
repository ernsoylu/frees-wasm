import { type SpreadsheetSpec } from './types'

const LOCAL_STORAGE_KEY = 'frees-spreadsheets'
export const SPREADSHEET_SAVE_ERROR_EVENT = 'SPREADSHEET_SAVE_ERROR_EVENT'

export function loadSpreadsheets(): SpreadsheetSpec[] {
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      if (Array.isArray(parsed)) {
        return parsed
      }
    }
  } catch (e) {
    console.warn('Failed to parse cached spreadsheets', e)
  }
  return []
}

export function saveSpreadsheets(specs: SpreadsheetSpec[]): boolean {
  try {
    localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(specs))
    return true
  } catch (e: unknown) {
    console.error('Failed to cache spreadsheets (quota exceeded?)', e)
    if (
      e instanceof DOMException &&
      (e.name === 'QuotaExceededError' || e.name === 'NS_ERROR_DOM_QUOTA_REACHED')
    ) {
      window.dispatchEvent(new CustomEvent(SPREADSHEET_SAVE_ERROR_EVENT, { detail: e }))
    }
    return false
  }
}

export function newSpreadsheet(count: number): SpreadsheetSpec {
  return {
    id: crypto.randomUUID(),
    name: `Spreadsheet ${count + 1}`,
    sheets: [], // The component will initialize it with emptySpreadsheetData() if needed
    autoSync: true,
  }
}
