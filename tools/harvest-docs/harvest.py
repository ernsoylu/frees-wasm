#!/usr/bin/env python3
"""Harvest candidate parity documents from the reference frontend docs.

The 2026-08-22 run (Wave D1 slice) over `../frees/frontend/src/docs/*.md`:
142 fenced blocks -> 129 candidates (1 already in the corpus verbatim) ->
44 solved by the Java oracle -> 43 promoted, 1 pending
(`docs_fluids_materials_03`, CO2 not in the linked rustprop fluid set).
`fixtures/README.md` ("Growing the corpus", item 2) records the outcome and
the classification of the 85 non-solving blocks.

Usage:
    tools/harvest-docs/harvest.py [docs-dir] [staging-dir]

Writes `<staging-dir>/<name>.frees` candidates; run the golden dumper over
that directory next:
    tools/golden-dumper/run.sh <staging-dir> <staging-golden-dir>
then review each golden (promotion rule in fixtures/README.md) before moving
anything into fixtures/. The filter is deliberately permissive — the oracle,
not this script, decides what is a document.
"""

import glob
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DOCS = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "..", "frees", "frontend", "src", "docs")
STAGE = sys.argv[2] if len(sys.argv) > 2 else os.path.join(REPO, "fixtures", "docs-harvest-staging")

BLOCK_KEYWORDS = re.compile(
    r"\b(DYNAMIC|COMPONENT|TABLE|CALL|PROCEDURE|FUNCTION|MODULE|DUPLICATE|"
    r"LINEARIZE|UncertaintyOf|connect)\b"
)


def norm(s: str) -> str:
    return re.sub(r"\s+", " ", s.strip()).lower()


def looks_like_document(body: str) -> bool:
    lines = [l for l in body.splitlines() if l.strip()]
    if not lines:
        return False
    for l in lines:
        ls = l.strip()
        # REPL sessions, shell transcripts and HTTP examples are never documents.
        if ls.startswith((">>", "»", "$ ", "curl ", "npm ", "docker ", "#!", "GET ", "POST ")):
            return False
    has_eq = any(re.search(r"^[^/{]*=", l) and "==" not in l for l in lines)
    return has_eq or bool(BLOCK_KEYWORDS.search("\n".join(lines)))


def main() -> None:
    os.makedirs(STAGE, exist_ok=True)
    existing = {
        norm(open(f).read())
        for f in glob.glob(os.path.join(REPO, "fixtures", "corpus", "*.frees"))
    }
    count = kept = dupes = 0
    for f in sorted(glob.glob(os.path.join(DOCS, "*.md"))):
        stem = os.path.basename(f)[:-3]
        text = open(f).read()
        for i, (lang, body) in enumerate(re.findall(r"```(\w*)\n(.*?)```", text, re.S), 1):
            count += 1
            if lang == "text":
                continue
            body = body.rstrip() + "\n"
            if not looks_like_document(body):
                continue
            if norm(body) in existing:
                dupes += 1
                continue
            name = f"docs_{stem}_{i:02d}.frees"
            with open(os.path.join(STAGE, name), "w") as out:
                out.write(body)
            existing.add(norm(body))
            kept += 1
    print(f"{count} blocks, {kept} candidates staged to {STAGE}, {dupes} corpus duplicates skipped")


if __name__ == "__main__":
    main()
