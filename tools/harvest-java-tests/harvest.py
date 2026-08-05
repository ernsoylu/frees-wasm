#!/usr/bin/env python3
"""Harvest candidate .frees documents from the Java reference test classes.

Extracts documents two ways:
  * Java text blocks (\"\"\"...\"\"\") — named from the enclosing method/constant.
  * The first string argument of solve(/parseResult( calls — literal
    concatenations, with String constants/locals in the same file resolved.

Writes them into fixtures/corpus-staged/corpus/ with a per-class kebab prefix,
skipping candidates identical (after trimming) to documents already in
fixtures/corpus/ or fixtures/corpus-pending/corpus/.

A manifest (harvest-manifest.json) records provenance per candidate, including
whether the solve call sat inside an assertThrows (i.e. the Java test documents
an error as the expected behaviour).
"""

import json
import os
import re
import sys

FREES_WASM = "/Users/erensoylu/homecloud/dev/frees-wasm"
JAVA_TESTS = (
    "/Users/erensoylu/homecloud/dev/frEES/backend/core/src/test/java/com/frees/backend/core"
)
OUT_CORPUS = os.path.join(FREES_WASM, "fixtures/corpus-staged/corpus")
MANIFEST = os.path.join(FREES_WASM, "tools/harvest-java-tests/harvest-manifest.json")

# class file -> (prefix, mode). mode "text" = prefer text-block extraction,
# "concat" = prefer solve-call extraction. Both run on every class; mode only
# decides which extraction claims a duplicate first (naming preference).
CLASSES = {
    "SystemDesignExamplesTest.java": ("sysdesign", "text"),
    "OdeProblemLibraryTest.java": ("odelib", "text"),
    "ProceduralFeaturesTest.java": ("proc", "text"),
    "CodeTableTest.java": ("codetable", "text"),
    "IntegralTest.java": ("integral", "text"),
    "MultiOutputDestructuringTest.java": ("multiout", "text"),
    "CurveFunctionTest.java": ("curvefn", "text"),
    "HvacExamplesTest.java": ("hvac", "text"),
    "EquationSystemSolverTest.java": ("eqsys", "concat"),
    "ControlSystemDesignTest.java": ("ctldesign", "concat"),
    "ControlSystemFrequencyTest.java": ("ctlfreq", "concat"),
    "ControlSystemTimeResponseTest.java": ("ctltime", "concat"),
    "LinearAlgebraCallTest.java": ("linalg", "concat"),
}

SIMPLE_ESCAPES = {
    "n": "\n",
    "t": "\t",
    "r": "\r",
    "f": "\f",
    "b": "\b",
    "s": " ",
    '"': '"',
    "'": "'",
    "\\": "\\",
    "0": "\0",
}


def decode_string(raw):
    """Decode the body of an ordinary Java string literal."""
    out = []
    i = 0
    while i < len(raw):
        c = raw[i]
        if c == "\\" and i + 1 < len(raw):
            nxt = raw[i + 1]
            if nxt == "u":
                j = i + 2
                while j < len(raw) and raw[j] == "u":
                    j += 1
                out.append(chr(int(raw[j : j + 4], 16)))
                i = j + 4
                continue
            out.append(SIMPLE_ESCAPES.get(nxt, nxt))
            i += 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def decode_text_block(raw):
    """Decode a Java text block body (content between the delimiters).

    raw starts right after the opening triple-quote line terminator and ends
    right before the closing triple-quote.
    """
    lines = raw.split("\n")
    # Closing delimiter position: if the last line is whitespace-only, the
    # closing quote was on its own line -> its indent participates and the
    # value ends with a newline.
    closing_own_line = lines and lines[-1].strip() == ""
    content_lines = lines[:-1] if closing_own_line else lines
    indents = [
        len(l) - len(l.lstrip()) for l in content_lines if l.strip() != ""
    ]
    if closing_own_line and lines:
        indents.append(len(lines[-1]))
    indent = min(indents) if indents else 0
    stripped = [l[indent:].rstrip() if l.strip() else "" for l in content_lines]
    value = "\n".join(stripped)
    if closing_own_line:
        value += "\n"
    # Escape processing (after incidental-whitespace removal, per JLS).
    value = value.replace("\\\n", "")  # line continuation
    out = []
    i = 0
    while i < len(value):
        c = value[i]
        if c == "\\" and i + 1 < len(value):
            out.append(SIMPLE_ESCAPES.get(value[i + 1], value[i + 1]))
            i += 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def tokenize(src):
    """Split Java source into segments: (kind, start, end, decoded_value).

    kinds: 'code', 'line_comment', 'block_comment', 'string', 'textblock',
    'char'. Positions are into src.
    """
    segs = []
    i = 0
    n = len(src)
    code_start = 0

    def flush_code(end):
        if end > code_start:
            segs.append(("code", code_start, end, src[code_start:end]))

    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            flush_code(i)
            j = src.find("\n", i)
            j = n if j == -1 else j
            segs.append(("line_comment", i, j, src[i:j]))
            i = j
            code_start = i
        elif c == "/" and i + 1 < n and src[i + 1] == "*":
            flush_code(i)
            j = src.find("*/", i + 2)
            j = n if j == -1 else j + 2
            segs.append(("block_comment", i, j, src[i:j]))
            i = j
            code_start = i
        elif src.startswith('"""', i):
            flush_code(i)
            # opening delimiter: """ then optional ws then newline
            j = i + 3
            while j < n and src[j] in " \t":
                j += 1
            assert src[j] == "\n", f"malformed text block at {i}"
            body_start = j + 1
            # find closing """ not preceded by backslash escape
            k = body_start
            while True:
                k = src.find('"""', k)
                assert k != -1, "unterminated text block"
                if src[k - 1] == "\\":
                    k += 1
                    continue
                break
            segs.append(
                ("textblock", i, k + 3, decode_text_block(src[body_start:k]))
            )
            i = k + 3
            code_start = i
        elif c == '"':
            flush_code(i)
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            segs.append(("string", i, j + 1, decode_string(src[i + 1 : j])))
            i = j + 1
            code_start = i
        elif c == "'":
            flush_code(i)
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == "'":
                    break
                j += 1
            segs.append(("char", i, j + 1, src[i : j + 1]))
            i = j + 1
            code_start = i
        else:
            i += 1
    flush_code(n)
    return segs


def blank_non_code(src, segs):
    """Return src with every non-code segment replaced by spaces (newlines kept)."""
    out = list(src)
    for kind, s, e, _ in segs:
        if kind != "code":
            for k in range(s, e):
                if out[k] != "\n":
                    out[k] = " "
    return "".join(out)


def kebab(name):
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1-\2", name)
    s = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1-\2", s)
    return s.replace("_", "-").lower()


class FileHarvest:
    def __init__(self, path):
        self.path = path
        self.src = open(path, encoding="utf-8").read()
        self.segs = tokenize(self.src)
        self.code = blank_non_code(self.src, self.segs)
        # anchors: (pos, name) for method decls and String constants
        self.anchors = []
        for m in re.finditer(r"\bvoid\s+(\w+)\s*\(", self.code):
            self.anchors.append((m.start(), m.group(1)))
        for m in re.finditer(r"\bstatic\s+final\s+String\s+(\w+)", self.code):
            self.anchors.append((m.start(), m.group(1)))
        self.anchors.sort()
        # string declarations: name -> list of (pos, value or None)
        self.decls = {}
        for m in re.finditer(r"\bString\s+(\w+)\s*=", self.code):
            eq = m.end() - 1
            end = self._top_level_semicolon(eq + 1)
            val = self._eval_expr(eq + 1, end)
            self.decls.setdefault(m.group(1), []).append((m.start(), val))

    def _top_level_semicolon(self, start):
        depth = 0
        i = start
        while i < len(self.code):
            c = self.code[i]
            if c in "([{":
                depth += 1
            elif c in ")]}":
                depth -= 1
            elif c == ";" and depth == 0:
                return i
            i += 1
        return len(self.code)

    def _literal_segments(self, start, end):
        return [
            (kind, s, e, v)
            for kind, s, e, v in self.segs
            if kind in ("string", "textblock") and s >= start and e <= end
        ]

    def _eval_expr(self, start, end, at=None):
        """Evaluate a Java expression span as a string concatenation.

        Allowed: string/textblock literals, '+', identifiers resolving to an
        earlier String declaration. Returns None if anything else appears.
        """
        pos = at if at is not None else start
        parts = []
        i = start
        lits = {s: (e, v) for kind, s, e, v in self._literal_segments(start, end)}
        while i < end:
            c = self.code[i]
            if i in lits:
                e, v = lits[i]
                parts.append(v)
                i = e
            elif c in " \t\n\r+":
                i += 1
            elif c.isalpha() or c == "_":
                m = re.match(r"[A-Za-z_]\w*", self.code[i:end])
                name = m.group(0)
                # method call or field access -> not a plain identifier
                rest = self.code[i + len(name) : end].lstrip()
                if rest.startswith("(") or rest.startswith("."):
                    return None
                cands = [
                    v
                    for p, v in self.decls.get(name, [])
                    if p < pos and v is not None
                ]
                if not cands:
                    return None
                parts.append(cands[-1])
                i += len(name)
            else:
                # spaces where literals were blanked
                if c == " ":
                    i += 1
                    continue
                return None
        return "".join(parts) if parts else None

    def enclosing(self, pos):
        name = None
        for p, n in self.anchors:
            if p < pos:
                name = n
            else:
                break
        return name or "top"

    def text_blocks(self):
        for kind, s, e, v in self.segs:
            if kind == "textblock":
                yield s, v

    def solve_calls(self):
        for m in re.finditer(r"\b(?:solve|parseResult)\s*\(", self.code):
            open_paren = m.end() - 1
            # balanced close
            depth = 0
            i = open_paren
            while i < len(self.code):
                if self.code[i] == "(":
                    depth += 1
                elif self.code[i] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                elif self.code[i] == "," and depth == 1:
                    break
                i += 1
            arg_start, arg_end = open_paren + 1, i
            val = self._eval_expr(arg_start, arg_end, at=m.start())
            in_throws = "assertThrows" in self.code[max(0, m.start() - 250) : m.start()]
            yield m.start(), val, in_throws


def main():
    os.makedirs(OUT_CORPUS, exist_ok=True)
    existing = {}
    for d in (
        os.path.join(FREES_WASM, "fixtures/corpus"),
        os.path.join(FREES_WASM, "fixtures/corpus-pending/corpus"),
    ):
        for f in os.listdir(d):
            if f.endswith(".frees"):
                content = open(os.path.join(d, f), encoding="utf-8").read().strip()
                existing[content] = f

    manifest = {}
    seen = {}  # trimmed content -> assigned name
    counts = {}
    skipped = {"fragment": 0, "dup_existing": 0, "dup_harvest": 0, "unresolved": 0}

    for fname, (prefix, mode) in CLASSES.items():
        path = os.path.join(JAVA_TESTS, fname)
        fh = FileHarvest(path)

        candidates = []  # (pos, doc, kind, in_throws)
        for pos, doc in fh.text_blocks():
            candidates.append((pos, doc, "textblock", False))
        for pos, doc, in_throws in fh.solve_calls():
            if doc is None:
                skipped["unresolved"] += 1
                continue
            candidates.append((pos, doc, "solvecall", in_throws))
        # extraction preference: preferred kind first, then position
        pref = "textblock" if mode == "text" else "solvecall"
        candidates.sort(key=lambda c: (0 if c[2] == pref else 1, c[0]))

        name_counts = {}
        for pos, doc, kind, in_throws in candidates:
            trimmed = doc.strip()
            lines = [l for l in trimmed.split("\n")]
            if "=" not in trimmed:
                skipped["fragment"] += 1
                continue
            if kind == "textblock" and len(lines) < 2:
                skipped["fragment"] += 1
                continue
            if trimmed in existing:
                skipped["dup_existing"] += 1
                continue
            if trimmed in seen:
                skipped["dup_harvest"] += 1
                continue
            method = fh.enclosing(pos)
            base = f"{prefix}-{kebab(method)}"
            n = name_counts.get(base, 0) + 1
            name_counts[base] = n
            name = base if n == 1 else f"{base}-{n}"
            seen[trimmed] = name
            counts[prefix] = counts.get(prefix, 0) + 1
            out = doc if doc.endswith("\n") else doc + "\n"
            with open(os.path.join(OUT_CORPUS, name + ".frees"), "w") as f:
                f.write(out)
            manifest[name] = {
                "class": fname,
                "method": method,
                "kind": kind,
                "in_assert_throws": in_throws,
            }

    with open(MANIFEST, "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)

    total = sum(counts.values())
    print(f"harvested {total} candidates:")
    for prefix in sorted(counts):
        print(f"  {prefix}: {counts[prefix]}")
    print(f"skipped: {skipped}")


if __name__ == "__main__":
    main()
