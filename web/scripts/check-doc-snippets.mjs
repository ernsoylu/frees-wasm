// Doc-snippet verification: every fenced block tagged ```run in src/docs/*.md
// must Check (structurally solvable) AND Solve (converge) against a live
// backend. These are the blocks the Help portal renders with a Run button, so
// this is the gate that keeps runnable documentation honest.
//
// Requires the backend at http://localhost:8080 (./frees.sh start).
// Run: node scripts/check-doc-snippets.mjs

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DOCS_DIR = path.join(__dirname, '../src/docs');
const API = process.env.FREES_API_BASE || 'http://localhost:8080';

const STOP = { maxIterations: 250, relativeResiduals: 1e-12, changeInVariables: 1e-15, elapsedTimeSeconds: 3600 };

// ── Extract ```run blocks, remembering the [Topic: id] they live under ───────
function extractSnippets() {
  const snippets = [];
  for (const file of fs.readdirSync(DOCS_DIR).filter((f) => f.endsWith('.md'))) {
    const lines = fs.readFileSync(path.join(DOCS_DIR, file), 'utf-8').split('\n');
    let topic = '';
    let inRun = false;
    let buf = [];
    let startLine = 0;
    for (let i = 0; i < lines.length; i++) {
      const t = lines[i].trim();
      const m = t.match(/^\[Topic:\s*([a-zA-Z0-9_-]+)\]/);
      if (m) topic = m[1];
      if (!inRun && (t === '```run' || t.startsWith('```run '))) { inRun = true; buf = []; startLine = i + 1; continue; }
      if (inRun && t.startsWith('```')) {
        inRun = false;
        snippets.push({ file, topic, line: startLine, code: buf.join('\n') });
        continue;
      }
      if (inRun) buf.push(lines[i]);
    }
    if (inRun) throw new Error(`${file}: unterminated \`\`\`run fence`);
  }
  return snippets;
}

async function post(pathname, body) {
  // The API rate-limits bursts (429); back off and retry rather than fail.
  for (let attempt = 0; ; attempt++) {
    const res = await fetch(API + pathname, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (res.status === 429 && attempt < 8) {
      await new Promise((r) => setTimeout(r, 1500 * (attempt + 1)));
      continue;
    }
    if (!res.ok) throw new Error(`${pathname} -> HTTP ${res.status}: ${(await res.text()).slice(0, 300)}`);
    return res.json();
  }
}

// Solve handles both server modes: a synchronous 200 result, or the async
// 202 { jobId, status: PENDING } that we poll to completion.
async function runSolve(text) {
  let data = await post('/api/solve', {
    text, stopCriteria: STOP, variableInfo: [], findAllSolutions: false,
    displayUnitSystem: 'SI', fillMissing: false, functionTables: [], overrides: [],
  });
  if (data.jobId && data.status === 'PENDING') {
    const deadline = Date.now() + 120_000;
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 700));
      const res = await fetch(`${API}/api/jobs/${encodeURIComponent(data.jobId)}`);
      if (res.status === 429) continue; // rate-limited poll: just wait another tick
      const s = await res.json();
      if (s.status === 'COMPLETED') return s.result;
      if (s.status === 'FAILED') throw new Error(`solve FAILED: ${s.error}`);
    }
    throw new Error('solve timed out after 120 s');
  }
  return data;
}

const snippets = extractSnippets();
if (!snippets.length) {
  console.log('doc-snippets: no ```run blocks found — nothing to verify.');
  process.exit(0);
}

// Fail fast if the backend isn't up: this script is meaningless without it.
try {
  await fetch(API + '/api/health');
} catch {
  console.error(`✗ backend not reachable at ${API} — start it with ./frees.sh start`);
  process.exit(1);
}

let failures = 0;
for (const s of snippets) {
  const label = `${s.file}[${s.topic}]:${s.line}`;
  await new Promise((r) => setTimeout(r, 700)); // stay under the API rate limit
  try {
    const chk = await post('/api/check', {
      text: s.code, variableInfo: [], stopCriteria: {}, functionTables: [], overrides: [],
    });
    if (!chk.solvable) throw new Error(`check: not solvable — ${chk.message}`);
    const sol = await runSolve(s.code);
    if (!sol.success) throw new Error(`solve: ${sol.error || 'not successful'}`);
    console.log(`  ✓ ${label} (${chk.equations} eq, max residual ${sol.stats?.maxResidual ?? '?'})`);
  } catch (e) {
    failures++;
    console.error(`  ✗ ${label}\n      ${String(e.message || e).replace(/\n/g, '\n      ')}`);
  }
}

console.log(`\ndoc-snippets: ${snippets.length - failures}/${snippets.length} runnable snippets check + solve.`);
if (failures) process.exit(1);
