// Full-text search index for the help system. Indexes every documentation
// topic, every reference catalog entry, and the examples library, then ranks
// matches so a single search box jumps straight to the relevant page.
//
// Ranking weights (higher = more relevant):
//   title/label match .... 100
//   keyword match ........ 80
//   heading (#) match .... 60
//   table/example match .. 30
//   body text match ...... 10
// Prefix matches score higher than substring matches; exact matches highest.

import { DOCS_CATALOG } from './docsCatalog';
import { EXAMPLES } from './examples';
import { REFERENCE_PAGES } from './referenceCatalog';
import {
  MATH_FUNCTIONS,
  CALL_PROCEDURES,
  MATRIX_FUNCTIONS,
  FLUID_PROPERTY_OUTPUTS,
  AIRH2O_OUTPUTS,
  MATERIAL_FUNCTIONS,
  SOLID_MATERIALS,
  TABLE_FUNCTIONS,
  PARAMETRIC_ACCESSORS,
  ODE_ACCESSORS,
  UTILITY_PROPERTY_FUNCS,
  CONSTANT_DESCRIPTIONS,
} from './helpReference';

/** Facet for filtering results by what kind of page they are. */
export type SearchKind = 'guide' | 'reference' | 'component' | 'example';

export interface SearchHit {
  /** Topic id to navigate to. */
  id: string;
  /** Display label for the result. */
  label: string;
  /** Section/category for context. */
  section: string;
  /** A short snippet showing the match in context. */
  snippet: string;
  /** Relevance score (higher = better). */
  score: number;
  /** Facet kind (guide page, reference page, component, example library). */
  kind: SearchKind;
}

// Search-term synonyms for engineers arriving from other tools: each term is
// expanded to the frees vocabulary and the best-scoring variant counts.
// Single words only (the query is split on whitespace before expansion).
const SYNONYMS: Record<string, string[]> = {
  // matrix-language / block-diagram vocabulary
  ode113: ['dynamic', 'ode45'],
  fsolve: ['newton', 'solve', 'guess'],
  fzero: ['newton', 'guess'],
  interp1: ['interpolate'],
  interp2: ['interpolate2d'],
  // equation-solver vocabulary
  duplicate: ['for', 'arrays'],
  procedure: ['function', 'call'],
  // Domain vocabulary
  humidity: ['airh2o', 'psychrometric'],
  refrigerant: ['coolprop', 'fluid'],
  transient: ['dynamic'],
  simulation: ['dynamic', 'components'],
};

interface IndexEntry {
  id: string;
  label: string;
  section: string;
  kind: SearchKind;
  /** Lowercased headings extracted from the markdown (## ...). */
  headings: string[];
  /** Lowercased keywords from the nav (added by the caller). */
  keywords: string[];
  /** Lowercased full text (markdown or catalog prose). */
  text: string;
}

// Extract ## / ### headings and strip markdown noise from a markdown blob.
function parseMarkdown(md: string): { headings: string[]; text: string } {
  const lines = md.split('\n');
  const headings: string[] = [];
  const cleaned: string[] = [];
  let inCode = false;
  for (const line of lines) {
    const t = line.trim();
    if (t.startsWith('```')) { inCode = !inCode; cleaned.push(t); continue; }
    if (!inCode && t.startsWith('#')) {
      headings.push(t.replace(/^#+\s*/, '').toLowerCase());
    }
    // keep code lines too — they contain real function names/syntax
    cleaned.push(line);
  }
  // strip common markdown markers for the text index
  const text = cleaned.join('\n')
    .replaceAll('```', ' ')
    .replace(/`([^`]*)`/g, '$1')   // inline code -> plain text
    .replace(/\$\$?([^$]*)\$\$?/g, '$1') // latex -> plain text
    .replace(/[#*_>|]/g, ' ')
    .toLowerCase();
  return { headings, text };
}

// Build the static index once. The caller merges in nav keywords per topic.
function buildIndex(): IndexEntry[] {
  const entries: IndexEntry[] = [];

  // 1. Markdown documentation topics.
  for (const [id, md] of Object.entries(DOCS_CATALOG)) {
    const { headings, text } = parseMarkdown(md);
    // Derive a label from the first H1.
    const h1 = md.match(/^#[ \t]+(\S.*)$/m);
    entries.push({
      id,
      label: h1 ? h1[1].trim() : id,
      section: 'Documentation',
      kind: 'guide',
      headings,
      keywords: [],
      text,
    });
  }

  // 2. Reference catalog topics — index their function/material rows so a
  //    search for "bode" or "enthalpy" lands on the right reference page.
  // The per-symbol reference pages (2b below) are the canonical search targets.
  // These topic entries are a belt-and-braces fallback that re-homes the full
  // symbol surface onto the A–Z index and the live data pages, so a name search
  // still resolves even for any symbol that lacks a dedicated page yet.
  const refTopics: { id: string; label: string; section: string; rows: { name: string; desc: string }[] }[] = [
    { id: 'ref-index', label: 'A–Z Function Index', section: 'Reference',
      rows: [
        ...MATH_FUNCTIONS.flatMap(g => g.functions), ...MATRIX_FUNCTIONS,
        ...CALL_PROCEDURES.map(p => ({ name: p.name, desc: p.desc })),
        ...MATERIAL_FUNCTIONS, ...SOLID_MATERIALS.map(m => ({ name: m, desc: '' })),
        ...TABLE_FUNCTIONS, ...PARAMETRIC_ACCESSORS, ...ODE_ACCESSORS,
      ] },
    { id: 'ref-fluids', label: 'Supported Fluids', section: 'Reference',
      rows: [...FLUID_PROPERTY_OUTPUTS, ...AIRH2O_OUTPUTS, ...UTILITY_PROPERTY_FUNCS] },
    { id: 'ref-units', label: 'Units & Constants', section: 'Reference',
      rows: CONSTANT_DESCRIPTIONS.map(c => ({ name: c.name, desc: c.desc })) },
  ];
  for (const t of refTopics) {
    const text = t.rows.map(r => `${r.name} ${r.desc}`).join('\n').toLowerCase();
    entries.push({ id: t.id, label: t.label, section: t.section, kind: 'reference', headings: [], keywords: [], text });
  }

  // 2b. Per-function reference pages — index name, summary, tags,
  //     and full body so a search for a function or a method/equation lands here.
  for (const p of REFERENCE_PAGES) {
    const { headings, text } = parseMarkdown(p.body);
    entries.push({
      id: 'refpage:' + p.slug,
      label: p.name,
      section: 'Reference · ' + p.category,
      kind: p.category.startsWith('Component') ? 'component' : 'reference',
      headings,
      keywords: [p.name.toLowerCase(), ...p.tags.map(t => t.toLowerCase())],
      text: `${p.summary} ${p.references.join(' ')} ${text}`.toLowerCase(),
    });
  }

  // 3. Examples library — index titles + descriptions so domain searches land there.
  const exText = EXAMPLES.map(e => `${e.title} ${e.description} ${e.category}`).join('\n').toLowerCase();
  entries.push({
    id: 'examples',
    label: 'Engineering Examples Library',
    section: 'Case Studies',
    kind: 'example',
    headings: [],
    keywords: [],
    text: exText,
  });

  return entries;
}

let cachedIndex: IndexEntry[] | null = null;
function getIndex(): IndexEntry[] {
  if (!cachedIndex) cachedIndex = buildIndex();
  return cachedIndex;
}

/** Merge nav keywords into the index. Called once with the CATEGORIES data. */
export function buildSearchIndex(
  navKeywords: Record<string, string[]>
): IndexEntry[] {
  const idx = getIndex();
  for (const e of idx) {
    e.keywords = (navKeywords[e.id] ?? []).map(k => k.toLowerCase());
  }
  return idx;
}

// Score a term (with its synonyms — best variant wins) against an entry.
function scoreTermExpanded(term: string, entry: IndexEntry): number {
  let best = scoreTerm(term, entry);
  for (const syn of SYNONYMS[term] ?? []) {
    const s = scoreTerm(syn, entry);
    if (s > best) best = s;
  }
  return best;
}

// Score a single term against an entry across all fields.
function scoreTerm(term: string, entry: IndexEntry): number {
  let score = 0;
  const label = entry.label.toLowerCase();

  // Label: exact > prefix > substring
  if (label === term) score += 100;
  else if (label.startsWith(term)) score += 70;
  else if (label.includes(term)) score += 40;

  // Keywords (strong signal — curated by hand)
  for (const kw of entry.keywords) {
    if (kw === term) score += 80;
    else if (kw.startsWith(term)) score += 50;
    else if (kw.includes(term)) score += 30;
  }

  // Headings
  for (const h of entry.headings) {
    if (h.includes(term)) score += 60;
  }

  // Body text — count occurrences, capped
  let occ = 0;
  let from = 0;
  while ((from = entry.text.indexOf(term, from)) !== -1) { occ++; from += term.length; if (occ > 8) break; }
  score += occ * 10;

  return score;
}

/** Extract a snippet around the first match of any term in the text. */
function snippet(terms: string[], text: string): string {
  if (terms.length === 0) return '';
  let bestPos = -1;
  let bestTerm = '';
  for (const term of terms) {
    const pos = text.indexOf(term);
    if (pos !== -1 && (bestPos === -1 || pos < bestPos)) { bestPos = pos; bestTerm = term; }
  }
  if (bestPos === -1) return '';
  const start = Math.max(0, bestPos - 40);
  const end = Math.min(text.length, bestPos + bestTerm.length + 60);
  let snip = text.slice(start, end).replace(/\s+/g, ' ').trim();
  if (start > 0) snip = '…' + snip;
  if (end < text.length) snip = snip + '…';
  return snip;
}

/**
 * Run a query against the index and return ranked hits.
 * The query is split into terms; an entry must match ALL terms (AND), scored
 * by the sum of per-term scores. Short queries (1-2 chars) use prefix-only
 * matching against labels/keywords to stay snappy and relevant.
 */
export function searchDocs(query: string, limit = 12, kind: SearchKind | null = null): SearchHit[] {
  const q = query.trim().toLowerCase();
  if (q.length < 2) return [];
  const idx = getIndex();
  const terms = q.split(/\s+/).filter(t => t.length > 0);

  const hits: SearchHit[] = [];
  for (const entry of idx) {
    if (kind && entry.kind !== kind) continue;
    let total = 0;
    let matchedAll = true;
    for (const term of terms) {
      const s = scoreTermExpanded(term, entry);
      if (s === 0) { matchedAll = false; break; }
      total += s;
    }
    if (!matchedAll) continue;
    hits.push({
      id: entry.id,
      label: entry.label,
      section: entry.section,
      snippet: snippet(terms, entry.text) || snippet(terms, entry.label.toLowerCase()),
      score: total,
      kind: entry.kind,
    });
  }
  hits.sort((a, b) => b.score - a.score);
  return hits.slice(0, limit);
}
