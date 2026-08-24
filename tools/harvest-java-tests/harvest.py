#!/usr/bin/env python3
"""Harvest candidate .frees documents from the Java reference test classes.

Extracts documents two ways:
  * Java text blocks (\"\"\"...\"\"\") — named from the enclosing method/constant.
  * The first string argument of solve(/solveAll(/solvePermissive(/check(/
    parseResult( calls — literal concatenations, with String constants/locals
    in the same file resolved.

Wave I grew the resolver over Phase 12's (see `docs/status-phase12.md`,
"did not deliver" item 1 — the representable-document boundary):

  * ``tmpl.formatted(args)`` / ``String.format(tmpl, args)`` evaluate when the
    arguments are literals (Java's default-locale ``%f`` rendering, which is
    what the tests rely on). Raw text blocks that still carry a conversion
    (``%f`` in the body) are skipped as templates instead of staged as broken
    documents — Phase 12 staged `sysdesign-ex15` that way and golden review
    had to drop it.
  * In-file helper methods are inlined: ``solve(chiller(0.02))`` binds the
    helper's parameters to the literal call arguments and evaluates its
    ``return`` expression; a solve *inside* a helper re-evaluates once per
    distinct literal-argument call site, named after the calling test method.
  * Cross-file constants resolve through a registry of every test class's
    ``static final String`` fields. (Measured empty at solve sites in the
    2026-08-24 inventory — the feature is kept because the registry is also
    what the inventory uses to prove that.)
  * ``Map.of(name, new ProcDef.FunctionTableDef(...))`` extra-defs arguments
    evaluate into a ``<name>.tables.json`` sidecar beside the candidate — the
    request-level Function Tables channel (Wave H / D10). `tools/golden-dumper`
    installs the sidecar on the Java side and embeds it in the fixture as
    `function_tables`; `tests/parity.rs` replays it through
    `solve_with_tables`. See `fixtures/README.md`.
  * ``.frees`` documents under test *resources* directories listed in
    RESOURCE_DIRS are copied through verbatim (the validation suite).

Writes candidates into the --out directory (default
fixtures/corpus-staged/corpus/) with a per-class kebab prefix, skipping
candidates identical (after trimming, with identical sidecar tables) to
documents already in fixtures/corpus/ or fixtures/corpus-pending/corpus/.
Assigned names also avoid the stems already in those directories, so a re-run
after promotion can never stage a different document under a promoted name.

A manifest (harvest-manifest.json) records provenance per candidate, including
whether the solve call sat inside an assertThrows (i.e. the Java test documents
an error as the expected behaviour). The manifest is merged, not overwritten:
entries for fixtures promoted from earlier harvests survive a re-run.

--inventory scans every test class (all packages, not just CLASSES) and prints
the per-class classification of solve-call documents — resolved vs the blocker
classes (a-tables / a-complex / a-specs / a-settings / b-format / c-crossfile /
unresolved) — without writing anything.
"""

import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FREES_WASM = os.path.dirname(os.path.dirname(HERE))


def _reference_repo():
    parent = os.path.dirname(FREES_WASM)
    for name in ("frees", "frEES"):
        cand = os.path.join(parent, name)
        if os.path.isdir(os.path.join(cand, "backend", "core")):
            return cand
    sys.exit("reference repo not found beside this one (../frees or ../frEES)")


JAVA_TEST_ROOT = os.path.join(
    _reference_repo(), "backend/core/src/test/java/com/frees/backend"
)
JAVA_TESTS = os.path.join(JAVA_TEST_ROOT, "core")
RESOURCE_ROOT = os.path.join(_reference_repo(), "backend/core/src/test/resources")
DEFAULT_OUT = os.path.join(FREES_WASM, "fixtures/corpus-staged/corpus")
MANIFEST = os.path.join(HERE, "harvest-manifest.json")

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
    # Wave I — the classes whose documents the extended resolver unlocked
    # (formatted templates and helper-method inlining; blocker class b).
    "TwoPhaseChargeTest.java": ("tpcharge", "concat"),
    "AcComponentsTest.java": ("accomp", "concat"),
    "ChillerTest.java": ("chiller", "concat"),
    "ChargeClosedCycleTest.java": ("chgclosed", "concat"),
    "TwoPhaseCycleTest.java": ("tpcycle", "concat"),
    "EpsNtuFloatingTest.java": ("epsntu", "concat"),
    "ChargeCycleTest.java": ("chgcycle", "concat"),
    "CallAutoSizeOutputTest.java": ("callauto", "concat"),
    "ComponentDomainSeparationTest.java": ("domsep", "concat"),
    "ClosedLoopDiagnosisTest.java": ("closedloop", "concat"),
}

# resource directory (relative to RESOURCE_ROOT) -> fixture prefix. Documents
# are copied verbatim; the stem keeps the file's own name.
RESOURCE_DIRS = {
    "validation": "validation",
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

# A conversion left in a document means an unformatted template leaked
# through — never a valid .frees document.
TEMPLATE_RE = re.compile(r"%[-+ #0-9.]*[fdse]\b")


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


class Unresolved(Exception):
    """An expression the evaluator cannot (or must not) evaluate.

    ``reason`` is one of: 'format' (a .formatted/String.format whose inputs do
    not evaluate), 'param' (an identifier that is a parameter of the enclosing
    method — resolvable through a literal-argument call site), 'crossfile' (a
    ClassName.CONST with no registry entry), 'method' (a call the evaluator
    does not model), 'ident' (an unknown identifier), 'other'.
    """

    def __init__(self, reason, names=()):
        super().__init__(f"{reason}: {sorted(names)}")
        self.reason = reason
        self.names = set(names)


class _StringClass:
    """Marker for the bare identifier `String` (so `.format` resolves)."""


STRING_CLASS = _StringClass()


class _ClassRef:
    """Marker for a bare capitalized identifier that names a class."""

    def __init__(self, name):
        self.name = name


class Ctor:
    """An evaluated `new Qualified.Type(args)` construction."""

    def __init__(self, type_name, args):
        self.type_name = type_name  # e.g. "ProcDef.Curve"
        self.args = args


def java_str(v):
    if isinstance(v, str):
        return v
    if isinstance(v, bool):
        return "true" if v else "false"
    if v is None:
        return "null"
    return str(v)


def java_format(fmt, args):
    """Java's default-locale `formatted` for the conversions the tests use.

    Python's %-formatting matches Java for %s/%d/%f/%e (and precision forms)
    on numeric arguments once Java's primitive widening is accounted for —
    which it is, because every such argument reaches the template through a
    `double` parameter or literal. %n becomes a newline.
    """
    fmt = fmt.replace("%n", "\n")
    try:
        return fmt % tuple(args)
    except (TypeError, ValueError) as exc:
        raise Unresolved("format") from exc


NUMBER_RE = re.compile(
    r"(?:0[xX][0-9a-fA-F_]+|(?:\d[\d_]*\.?[\d_]*|\.\d[\d_]*)(?:[eE][+-]?\d+)?[fFdDlL]?)"
)
IDENT_RE = re.compile(r"[A-Za-z_]\w*")
CONTROL_KEYWORDS = {
    "if", "for", "while", "switch", "catch", "return", "new", "synchronized",
    "assert", "throw", "else", "do", "try",
}
DECL_RE = re.compile(
    r"\b(?:String|var|double|int|boolean|long|float|SolverSettings|"
    r"Map<[^;={}()]*>)\s+(\w+)\s*=(?!=)"
)
METHOD_RE = re.compile(r"\b(\w+)\s*\(([^()]*)\)\s*\{")


class FileHarvest:
    def __init__(self, path, registry=None):
        self.path = path
        self.src = open(path, encoding="utf-8").read()
        self.segs = tokenize(self.src)
        self.code = blank_non_code(self.src, self.segs)
        self.registry = registry or {}
        # literal segments by start position
        self.lit = {
            s: (e, v)
            for kind, s, e, v in self.segs
            if kind in ("string", "textblock")
        }
        # declarations: name -> list of (pos, expr_start, expr_end), lazy
        self.decls = {}
        for m in DECL_RE.finditer(self.code):
            eq = m.end() - 1
            end = self._top_level_semicolon(eq + 1)
            self.decls.setdefault(m.group(1), []).append((m.start(), eq + 1, end))
        # methods: name -> list of dicts. Body brace-matched from the header.
        self.methods = {}
        for m in METHOD_RE.finditer(self.code):
            name = m.group(1)
            if name in CONTROL_KEYWORDS:
                continue
            stmt_start = max(
                self.code.rfind(";", 0, m.start()),
                self.code.rfind("{", 0, m.start()),
                self.code.rfind("}", 0, m.start()),
            )
            head = self.code[stmt_start + 1 : m.start()]
            # JUnit test methods are package-private (`@Test void name()`), so
            # `void` must anchor too, not just the access modifiers.
            if not re.search(r"\b(?:private|public|protected|static|void)\b", head):
                continue
            params = []
            ptext = m.group(2).strip()
            ok = True
            if ptext:
                for part in ptext.split(","):
                    words = part.split()
                    if len(words) < 2:
                        ok = False
                        break
                    params.append(words[-1])
            if not ok:
                continue
            body_start = m.end()  # just past '{'
            body_end = self._match_brace(body_start)
            self.methods.setdefault(name, []).append(
                {
                    "pos": m.start(),
                    "params": params,
                    "body": (body_start, body_end),
                }
            )
        # anchors: (pos, name) for method decls and String constants
        self.anchors = []
        for name, decls in self.methods.items():
            for d in decls:
                self.anchors.append((d["pos"], name))
        for m in re.finditer(r"\bstatic\s+final\s+String\s+(\w+)", self.code):
            self.anchors.append((m.start(), m.group(1)))
        self.anchors.sort()

    def _match_brace(self, start):
        depth = 1
        i = start
        while i < len(self.code):
            c = self.code[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    return i
            i += 1
        return len(self.code)

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

    def constants(self):
        """{NAME: value} for every static final String that evaluates."""
        out = {}
        for m in re.finditer(r"\bstatic\s+final\s+String\s+(\w+)\s*=", self.code):
            name = m.group(1)
            eq = m.end() - 1
            end = self._top_level_semicolon(eq + 1)
            try:
                v = self.eval_span(eq + 1, end)
            except Unresolved:
                continue
            if isinstance(v, str):
                out[name] = v
        return out

    def enclosing(self, pos):
        name = None
        for p, n in self.anchors:
            if p < pos:
                name = n
            else:
                break
        return name or "top"

    def enclosing_method(self, pos):
        """The (name, decl) of the method whose body contains pos, or None."""
        for name, decls in self.methods.items():
            for d in decls:
                s, e = d["body"]
                if s <= pos < e:
                    return name, d
        return None

    # ------------------------------------------------------------------ eval

    def eval_span(self, start, end, bindings=None):
        v, i = self._expr(start, end, bindings or {})
        i = self._ws(i, end)
        if i < end:
            raise Unresolved("other")
        return v

    def _ws(self, i, end):
        # Literal segments are blanked to spaces in `code`; their *start*
        # positions are meaningful tokens, so whitespace-skipping must stop
        # there (the pre-Wave-I evaluator checked literals before whitespace).
        while i < end and self.code[i] in " \t\n\r" and i not in self.lit:
            i += 1
        return i

    def _expr(self, i, end, b):
        v, i = self._unary(i, end, b)
        while True:
            j = self._ws(i, end)
            if (
                j < end
                and self.code[j] == "+"
                and (j + 1 >= end or self.code[j + 1] != "+")
            ):
                rhs, i = self._unary(j + 1, end, b)
                if isinstance(v, str) or isinstance(rhs, str):
                    v = java_str(v) + java_str(rhs)
                else:
                    v = v + rhs
            else:
                return v, i

    def _unary(self, i, end, b):
        i = self._ws(i, end)
        if i < end and self.code[i] == "-":
            v, i = self._postfix(i + 1, end, b)
            if isinstance(v, (int, float)) and not isinstance(v, bool):
                return -v, i
            raise Unresolved("other")
        return self._postfix(i, end, b)

    def _postfix(self, i, end, b):
        v, i = self._primary(i, end, b)
        while True:
            j = self._ws(i, end)
            if j >= end or self.code[j] != ".":
                return v, i
            m = IDENT_RE.match(self.code, j + 1)
            if not m:
                raise Unresolved("other")
            name = m.group(0)
            k = self._ws(m.end(), end)
            if k < end and self.code[k] == "(":
                args, i = self._args(k, end, b)
                if name == "formatted" and isinstance(v, str):
                    v = java_format(v, args)
                elif name == "format" and v is STRING_CLASS:
                    if not args or not isinstance(args[0], str):
                        raise Unresolved("format")
                    v = java_format(args[0], args[1:])
                elif (
                    name == "of"
                    and isinstance(v, _ClassRef)
                    and v.name in ("Map", "List")
                ):
                    if v.name == "Map":
                        if len(args) % 2:
                            raise Unresolved("other")
                        v = list(zip(args[::2], args[1::2]))
                    else:
                        v = list(args)
                else:
                    raise Unresolved("method", [name])
            elif isinstance(v, _ClassRef):
                consts = self.registry.get(v.name)
                if consts is not None and name in consts:
                    v = consts[name]
                    i = m.end()
                else:
                    raise Unresolved("crossfile", [f"{v.name}.{name}"])
            else:
                raise Unresolved("method", [name])

    def _args(self, i, end, b):
        """Parse '(' args ')' starting at the open paren. Returns (args, pos)."""
        assert self.code[i] == "("
        args = []
        i = self._ws(i + 1, end)
        if i < end and self.code[i] == ")":
            return args, i + 1
        while True:
            v, i = self._expr(i, end, b)
            args.append(v)
            i = self._ws(i, end)
            if i < end and self.code[i] == ",":
                i += 1
                continue
            if i < end and self.code[i] == ")":
                return args, i + 1
            raise Unresolved("other")

    def _primary(self, i, end, b):
        i = self._ws(i, end)
        if i >= end:
            raise Unresolved("other")
        if i in self.lit:
            e, v = self.lit[i]
            return v, min(e, end)
        c = self.code[i]
        if c == "(":
            v, i = self._expr(i + 1, end, b)
            i = self._ws(i, end)
            if i < end and self.code[i] == ")":
                return v, i + 1
            raise Unresolved("other")
        if c.isdigit() or c == ".":
            m = NUMBER_RE.match(self.code, i)
            if not m:
                raise Unresolved("other")
            text = m.group(0).replace("_", "")
            suffix = text[-1] if text[-1] in "fFdDlL" else ""
            if suffix:
                text = text[:-1]
            if text.lower().startswith("0x"):
                return int(text, 16), m.end()
            if "." in text or "e" in text.lower() or suffix in "fFdD":
                return float(text), m.end()
            return int(text), m.end()
        m = IDENT_RE.match(self.code, i)
        if not m:
            raise Unresolved("other")
        name = m.group(0)
        i2 = m.end()
        if name == "new":
            return self._new(i2, end, b)
        if name == "true":
            return True, i2
        if name == "false":
            return False, i2
        if name == "null":
            return None, i2
        if name in b:
            return b[name], i2
        j = self._ws(i2, end)
        if j < end and self.code[j] == "(" and name not in ("String", "Map", "List"):
            # in-file helper call — inline it
            args, i3 = self._args(j, end, b)
            return self._inline(name, args), i3
        # a local/field declaration visible before this use
        decl = self._nearest_decl(name, m.start())
        if decl is not None:
            _, es, ee = decl
            return self.eval_span(es, ee, b), i2
        if name == "String":
            return STRING_CLASS, i2
        if name[0].isupper() and j < end and self.code[j] == ".":
            return _ClassRef(name), i2
        # a parameter of the enclosing method? resolvable via call sites
        enc = self.enclosing_method(m.start())
        if enc is not None and name in enc[1]["params"]:
            raise Unresolved("param", [name])
        raise Unresolved("ident", [name])

    def _nearest_decl(self, name, before):
        best = None
        for d in self.decls.get(name, []):
            if d[0] < before and (best is None or d[0] > best[0]):
                best = d
        return best

    def _new(self, i, end, b):
        i = self._ws(i, end)
        m = re.match(r"[\w.]+", self.code[i:end])
        if not m:
            raise Unresolved("other")
        type_name = m.group(0)
        i = i + m.end()
        i = self._ws(i, end)
        if i < end and self.code[i] == "[":
            # new double[]{...} / new double[] {...}
            j = self.code.find("]", i)
            if j == -1 or j >= end:
                raise Unresolved("other")
            i = self._ws(j + 1, end)
            if i >= end or self.code[i] != "{":
                raise Unresolved("other")
            close = self._match_brace_from(i)
            vals, k = [], i + 1
            while True:
                k = self._ws(k, close)
                if k >= close:
                    break
                v, k = self._expr(k, close, b)
                vals.append(v)
                k = self._ws(k, close)
                if k < close and self.code[k] == ",":
                    k += 1
            return [float(v) for v in vals], close + 1
        if i < end and self.code[i] == "(":
            args, i = self._args(i, end, b)
            return Ctor(type_name, args), i
        raise Unresolved("other")

    def _match_brace_from(self, open_pos):
        depth = 0
        i = open_pos
        while i < len(self.code):
            c = self.code[i]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    return i
            i += 1
        raise Unresolved("other")

    def _inline(self, name, args):
        decls = self.methods.get(name)
        if not decls:
            raise Unresolved("method", [name])
        last_err = None
        for d in decls:
            if len(d["params"]) != len(args):
                continue
            bindings = dict(zip(d["params"], args))
            s, e = d["body"]
            # the method's first top-level return statement
            depth = 0
            k = s
            ret = None
            while k < e:
                c = self.code[k]
                if c in "([{":
                    depth += 1
                elif c in ")]}":
                    depth -= 1
                elif depth == 0 and self.code.startswith("return", k):
                    ret = k
                    break
                k += 1
            if ret is None:
                raise Unresolved("method", [name])
            expr_start = ret + len("return")
            expr_end = self._top_level_semicolon(expr_start)
            try:
                return self.eval_span(expr_start, expr_end, bindings)
            except Unresolved as u:
                last_err = u
        raise last_err or Unresolved("method", [name])

    # -------------------------------------------------------------- documents

    def text_blocks(self):
        """Text blocks that are documents, not format templates.

        A block immediately followed by `.formatted(` (directly or through a
        closing paren) is a template — the solve-call path harvests its
        substituted form instead. So is any block still carrying a `%`
        conversion with a `.formatted` nearby.
        """
        for kind, s, e, v in self.segs:
            if kind != "textblock":
                continue
            j = self._ws(e, len(self.code))
            if j < len(self.code) and self.code[j] == ")":
                j = self._ws(j + 1, len(self.code))
            if self.code.startswith(".formatted", j):
                continue
            # An operand of a `+` concatenation is a fragment of the real
            # document (`REL + """tail"""`) — the solve-call path harvests
            # the concatenated whole; staging the part would golden an
            # unknown-component artifact.
            if j < len(self.code) and self.code[j] == "+":
                continue
            k = s - 1
            while k >= 0 and self.code[k] in " \t\n\r":
                k -= 1
            if k >= 0 and self.code[k] == "+":
                continue
            yield s, v

    CALL_RE = re.compile(
        r"\b(solve|solveAll|solvePermissive|parseResult|check)\s*\("
    )

    def _is_call(self, pos):
        """Distinguish `x.solve(...)` / `solve(...)` calls from declarations."""
        k = pos - 1
        while k >= 0 and self.code[k] in " \t\n\r":
            k -= 1
        if k < 0:
            return False
        c = self.code[k]
        if c in ".([{,;=+!&|?:}<>-":
            return True
        # preceding word: `return solve(...)` is a call; `Double> solve(` is not
        m = re.search(r"(\w+)\s*$", self.code[: k + 1])
        return bool(m) and m.group(1) == "return"

    def _split_args(self, open_paren):
        """Spans of every top-level argument of the call at open_paren."""
        spans = []
        depth = 0
        i = open_paren
        start = open_paren + 1
        while i < len(self.code):
            c = self.code[i]
            if c in "([{":
                depth += 1
            elif c in ")]}":
                depth -= 1
                if depth == 0:
                    if self.code[start:i].strip() or spans:
                        spans.append((start, i))
                    return spans, i
            elif c == "," and depth == 1:
                spans.append((start, i))
                start = i + 1
            i += 1
        return spans, len(self.code)

    def solve_calls(self):
        """Yield candidate documents from solver-entry calls.

        Each yield: (naming_pos, doc, in_throws, tables, tags)
          naming_pos — where to derive the fixture name from (the call site of
            the enclosing helper when parameter binding was needed);
          doc — the document text, or None when unresolvable;
          tables — the evaluated extra-defs tables (list, possibly empty);
          tags — blocker/classification tags for the inventory.
        """
        for m in self.CALL_RE.finditer(self.code):
            callee = m.group(1)
            open_paren = m.end() - 1
            if not self._is_call(m.start()):
                continue
            spans, _ = self._split_args(open_paren)
            if not spans:
                continue
            in_throws = (
                "assertThrows" in self.code[max(0, m.start() - 250) : m.start()]
            )
            tags = self._extra_arg_tags(callee, spans)
            tables, tables_err = self._tables_of(callee, spans)
            if tables:
                tags.append("a-tables")
            elif tables_err:
                tags.append("a-tables-unresolved")

            a0s, a0e = spans[0]
            arg_text = self.code[a0s:a0e]
            uses_format = "formatted" in arg_text or "String.format" in arg_text
            try:
                doc = self.eval_span(a0s, a0e, None)
            except Unresolved as u:
                if u.reason == "param":
                    yield from self._via_call_sites(
                        m.start(), (a0s, a0e), in_throws, tables, tags
                    )
                    continue
                if u.reason == "format" or uses_format:
                    tags.append("b-computed")
                elif u.reason == "crossfile":
                    tags.append("c-unresolved")
                yield m.start(), None, in_throws, tables, tags + [
                    f"unresolved-{u.reason}"
                ]
                continue
            if not isinstance(doc, str):
                continue
            if uses_format:
                tags.append("b-resolved")
            yield m.start(), doc, in_throws, tables, tags

    def _via_call_sites(self, call_pos, arg_span, in_throws, tables, tags):
        """Re-evaluate a param-blocked solve through its helper's call sites."""
        enc = self.enclosing_method(call_pos)
        if enc is None:
            yield call_pos, None, in_throws, tables, tags + ["unresolved-param"]
            return
        name, decl = enc
        for m in re.finditer(rf"\b{re.escape(name)}\s*\(", self.code):
            if decl["pos"] <= m.start() < decl["body"][1]:
                continue  # the declaration itself (or recursion)
            if not self._is_call(m.start()):
                continue
            spans, _ = self._split_args(m.end() - 1)
            if len(spans) != len(decl["params"]):
                continue
            try:
                args = [self.eval_span(s, e, None) for s, e in spans]
                bindings = dict(zip(decl["params"], args))
                doc = self.eval_span(arg_span[0], arg_span[1], bindings)
            except Unresolved as u:
                yield m.start(), None, in_throws, tables, tags + [
                    "b-helper",
                    f"unresolved-{u.reason}",
                ]
                continue
            if isinstance(doc, str):
                yield m.start(), doc, in_throws, tables, tags + ["b-helper"]

    def _extra_arg_tags(self, callee, spans):
        tags = []
        texts = [self.code[s:e].strip() for s, e in spans]
        if callee in ("solve", "solveAll", "solvePermissive"):
            if len(spans) >= 2 and texts[1] != "SolverSettings.DEFAULTS":
                tags.append(
                    "a-complex"
                    if self._is_complex_settings(spans[1])
                    else "a-settings"
                )
            if len(spans) >= 3 and texts[2] != "Map.of()":
                tags.append("a-specs")
        elif callee == "check" and len(spans) >= 2:
            if texts[1] not in ("false", "true"):
                tags.append("a-settings")
        return tags

    def _is_complex_settings(self, span):
        """Does the settings argument trace to `new SolverSettings(..., true)`?"""
        text = self.code[span[0] : span[1]].strip()
        m = re.fullmatch(r"\w+", text)
        if m:
            d = self._nearest_decl(text, span[0])
            if d is None:
                return False
            text = self.code[d[1] : d[2]].strip()
        if not re.match(r"new\s+SolverSettings\s*\(", text):
            return False
        return bool(re.search(r",\s*true\s*\)\s*$", text))

    def _tables_of(self, callee, spans):
        """The extra-defs argument evaluated into sidecar table dicts."""
        idx = None
        if callee in ("solve", "solveAll", "solvePermissive") and len(spans) >= 4:
            idx = 3
        elif callee == "check" and len(spans) >= 3:
            idx = 2
        if idx is None:
            return [], False
        s, e = spans[idx]
        if self.code[s:e].strip() == "Map.of()":
            return [], False
        try:
            v = self.eval_span(s, e, None)
        except Unresolved:
            return [], True
        return self._as_tables(v)

    def _as_tables(self, v):
        """Convert an evaluated Map.of(...) of FunctionTableDefs to dicts.

        Mirrors `SolveDtos.functionDefsOf`: keyed (and named) by the trimmed,
        lowercased table name; curve points are used exactly as constructed
        (the tests build `ProcDef.Curve` directly, already ascending in x).
        """
        if not isinstance(v, list):
            return [], True
        tables = []
        for entry in v:
            if not (isinstance(entry, tuple) and len(entry) == 2):
                return [], True
            _, ctor = entry
            if not (
                isinstance(ctor, Ctor)
                and ctor.type_name.endswith("FunctionTableDef")
                and len(ctor.args) == 5
            ):
                return [], True
            name, arg_names, x_log, y_log, curves = ctor.args
            if not isinstance(name, str) or not isinstance(arg_names, list):
                return [], True
            out_curves = []
            for c in curves if isinstance(curves, list) else []:
                if not (
                    isinstance(c, Ctor)
                    and c.type_name.endswith("Curve")
                    and len(c.args) == 3
                ):
                    return [], True
                param, xs, ys = c.args
                out_curves.append(
                    {
                        "param": float(param) if param is not None else None,
                        "xs": [float(x) for x in xs],
                        "ys": [float(y) for y in ys],
                    }
                )
            if not out_curves:
                return [], True
            tables.append(
                {
                    "name": name.strip().lower(),
                    "arg_names": [str(a) for a in arg_names],
                    "x_log": bool(x_log),
                    "y_log": bool(y_log),
                    "curves": out_curves,
                }
            )
        return tables, False


def build_registry(paths):
    """ClassName -> {CONST: value} over every test source, for cross-file refs."""
    registry = {}
    for p in paths:
        cls = os.path.basename(p).removesuffix(".java")
        try:
            fh = FileHarvest(p)
        except AssertionError:
            continue
        consts = fh.constants()
        if consts:
            registry[cls] = consts
    return registry


def all_test_sources():
    out = []
    for root, _dirs, files in os.walk(JAVA_TEST_ROOT):
        for f in sorted(files):
            if f.endswith(".java"):
                out.append(os.path.join(root, f))
    return out


def existing_documents():
    """(trimmed content, tables json or None) -> stem, plus the stem set."""
    existing = {}
    stems = set()
    for d in (
        os.path.join(FREES_WASM, "fixtures/corpus"),
        os.path.join(FREES_WASM, "fixtures/corpus-pending/corpus"),
    ):
        if not os.path.isdir(d):
            continue
        for f in os.listdir(d):
            if f.endswith(".frees"):
                stem = f.removesuffix(".frees")
                stems.add(stem)
                content = open(os.path.join(d, f), encoding="utf-8").read().strip()
                sidecar = os.path.join(d, stem + ".tables.json")
                tables = None
                if os.path.exists(sidecar):
                    tables = json.dumps(
                        json.load(open(sidecar, encoding="utf-8")), sort_keys=True
                    )
                existing[(content, tables)] = stem
    return existing, stems


def line_of(src, pos):
    return src[:pos].count("\n") + 1


def inventory():
    paths = all_test_sources()
    registry = build_registry(paths)
    totals = {}
    examples = {}
    print(f"{len(paths)} test classes under {JAVA_TEST_ROOT}\n")
    for p in paths:
        try:
            fh = FileHarvest(p, registry)
        except AssertionError as exc:
            print(f"  {os.path.basename(p)}: tokenizer failed ({exc})")
            continue
        rows = []
        for pos, doc, _thr, _tables, tags in fh.solve_calls():
            cls = "resolved" if doc is not None else "unresolved"
            for t in tags:
                totals[t] = totals.get(t, 0) + 1
                examples.setdefault(
                    t, f"{os.path.basename(p)}:{line_of(fh.src, pos)}"
                )
            totals[cls] = totals.get(cls, 0) + 1
            rows.append((cls, tags))
        blocks = list(fh.text_blocks())
        totals["textblocks"] = totals.get("textblocks", 0) + len(blocks)
        n_res = sum(1 for c, _ in rows if c == "resolved")
        n_un = len(rows) - n_res
        if rows or blocks:
            flat = sorted({t for _, tags in rows for t in tags})
            print(
                f"  {os.path.basename(p):46} calls {len(rows):3} "
                f"(ok {n_res:3} / un {n_un:3})  blocks {len(blocks):3}  {flat}"
            )
    print("\ntotals:")
    for k in sorted(totals):
        print(f"  {k:22} {totals[k]:4}  {examples.get(k, '')}")


def main():
    if "--inventory" in sys.argv:
        inventory()
        return
    out_corpus = DEFAULT_OUT
    if "--out" in sys.argv:
        out_corpus = sys.argv[sys.argv.index("--out") + 1]
    os.makedirs(out_corpus, exist_ok=True)
    existing, existing_stems = existing_documents()

    registry = build_registry(all_test_sources())

    manifest = {}
    if os.path.exists(MANIFEST):
        manifest = json.load(open(MANIFEST, encoding="utf-8"))
    seen = {}  # (trimmed content, tables json) -> assigned name
    counts = {}
    skipped = {
        "fragment": 0,
        "template": 0,
        "dup_existing": 0,
        "dup_harvest": 0,
        "unresolved": 0,
    }

    def assign_name(base, name_counts):
        n = name_counts.get(base, 0)
        while True:
            n += 1
            name = base if n == 1 else f"{base}-{n}"
            if name not in existing_stems:
                name_counts[base] = n
                return name

    def write_candidate(
        prefix, name_counts, pos_name, doc, kind, in_throws, tables, cls
    ):
        trimmed = doc.strip()
        if "=" not in trimmed:
            skipped["fragment"] += 1
            return
        if kind == "textblock" and len(trimmed.split("\n")) < 2:
            skipped["fragment"] += 1
            return
        if TEMPLATE_RE.search(trimmed):
            skipped["template"] += 1
            return
        tables_key = json.dumps(tables, sort_keys=True) if tables else None
        key = (trimmed, tables_key)
        if key in existing:
            skipped["dup_existing"] += 1
            return
        if key in seen:
            skipped["dup_harvest"] += 1
            return
        base = f"{prefix}-{kebab(pos_name)}"
        name = assign_name(base, name_counts)
        seen[key] = name
        counts[prefix] = counts.get(prefix, 0) + 1
        out = doc if doc.endswith("\n") else doc + "\n"
        with open(os.path.join(out_corpus, name + ".frees"), "w") as f:
            f.write(out)
        entry = {
            "class": cls,
            "method": pos_name,
            "kind": kind,
            "in_assert_throws": in_throws,
        }
        if tables:
            with open(os.path.join(out_corpus, name + ".tables.json"), "w") as f:
                json.dump(tables, f, indent=1)
                f.write("\n")
            entry["function_tables"] = [t["name"] for t in tables]
        manifest[name] = entry

    for fname, (prefix, mode) in CLASSES.items():
        path = os.path.join(JAVA_TESTS, fname)
        fh = FileHarvest(path, registry)

        candidates = []  # (pos, doc, kind, in_throws, tables)
        tabled_contents = set()
        for pos, doc, in_throws, tables, _tags in fh.solve_calls():
            if doc is None:
                skipped["unresolved"] += 1
                continue
            candidates.append((pos, doc, "solvecall", in_throws, tables))
            if tables:
                tabled_contents.add(doc.strip())
        for pos, doc in fh.text_blocks():
            # A text block whose content a table-carrying solve call also
            # produced is the same Java document — the tables belong to it;
            # staging a tables-less twin would golden an artifact error.
            if doc.strip() in tabled_contents:
                skipped["dup_harvest"] += 1
                continue
            candidates.append((pos, doc, "textblock", False, []))
        # extraction preference: preferred kind first, then position
        pref = "textblock" if mode == "text" else "solvecall"
        candidates.sort(key=lambda c: (0 if c[2] == pref else 1, c[0]))

        name_counts = {}
        for pos, doc, kind, in_throws, tables in candidates:
            write_candidate(
                prefix,
                name_counts,
                fh.enclosing(pos),
                doc,
                kind,
                in_throws,
                tables,
                fname,
            )

    for rel, prefix in RESOURCE_DIRS.items():
        d = os.path.join(RESOURCE_ROOT, rel)
        name_counts = {}
        for f in sorted(os.listdir(d)):
            if not f.endswith(".frees"):
                continue
            doc = open(os.path.join(d, f), encoding="utf-8").read()
            write_candidate(
                prefix,
                name_counts,
                f.removesuffix(".frees"),
                doc,
                "resource",
                False,
                [],
                os.path.join(rel, f),
            )

    with open(MANIFEST, "w") as f:
        json.dump(manifest, f, indent=2, sort_keys=True)

    total = sum(counts.values())
    print(f"harvested {total} candidates into {out_corpus}:")
    for prefix in sorted(counts):
        print(f"  {prefix}: {counts[prefix]}")
    print(f"skipped: {skipped}")


if __name__ == "__main__":
    main()
