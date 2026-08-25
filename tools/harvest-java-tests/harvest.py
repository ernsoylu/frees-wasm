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

Wave Q built the second sidecar, on the Wave-I pattern: the *solver request*.
``new SolverSettings(...)`` and ``Map.of(name, new VariableSpec(...))``
arguments evaluate into a ``<name>.request.json`` beside the candidate,
carrying the non-default stop criteria (complex mode included) and the
per-variable guesses/bounds/uncertainty the Java test handed
``EquationSystemSolver.solve(source, settings, specs, defs)``. The dumper
rebuilds the same two arguments, records what the Java produced **under those
settings**, and embeds the sidecar verbatim as `request`; `tests/parity.rs`
replays it through `solve_with_tables` with the matching `SolverSettings` and
`VariableOverride`s. An absent sidecar is the engine default and the empty
override slice — byte-for-byte the old call.

Wave J made the class selection automatic. Wave I's own inventory found that
the remaining growth was not a representability problem at all: 115 of the 138
document-bearing test classes were simply never listed in CLASSES. **Every test
class under JAVA_TEST_ROOT is now swept**; CLASSES only pins the fixture prefix
(and the extraction-preference mode) for the classes harvested before Wave J,
SKIP_CLASSES names the ones deliberately left out, and the sites the inventory
classified as unrepresentable — complex mode, ``VariableSpec`` overrides,
non-default solver settings — are dropped by tag (SKIP_SITE_TAGS) rather than
staged as documents whose default-settings golden would not be what the Java
test asserted. *(Wave Q carries most of those in the request sidecar above;
what still drops by tag is what the evaluator cannot turn into a request.)*

Writes candidates into the --out directory (default
fixtures/corpus-staged/corpus/) with a per-class kebab prefix, skipping
candidates identical (after trimming, **with identical sidecars** — the same
text under a different request is a different document) to documents already
in fixtures/corpus/ or fixtures/corpus-pending/corpus/. Assigned names also
avoid the stems already in those directories, so a re-run after promotion can
never stage a different document under a promoted name.

A manifest (harvest-manifest.json) records provenance per candidate, including
whether the solve call sat inside an assertThrows (i.e. the Java test documents
an error as the expected behaviour). The manifest is merged, not overwritten:
entries for fixtures promoted from earlier harvests survive a re-run.

--inventory scans every test class (all packages, not just CLASSES) and prints
the per-class classification of solve-call documents — resolved vs the blocker
classes (a-tables / a-complex / a-specs / a-settings / b-format / c-crossfile /
unresolved), the sites a request sidecar now carries (a-request, with
a-request-complex / -settings / -specs), and how much of the remaining
a-settings / a-specs residue is `*-alien`: a `solve(` that CALL_RE matched but
that is not `EquationSystemSolver.solve` at all (`CasIdentity.solve`, the
sparse `solve(double[])`). Writes nothing.
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
#
# Since Wave J this table is *not* the sweep: every class under JAVA_TEST_ROOT
# is harvested (see `swept_classes`), and an entry here only overrides the
# prefix a class's own name would otherwise derive. Keeping the pre-Wave-J
# nicknames is what stops a re-run restaging the whole promoted corpus under
# new stems.
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

# Classes the sweep deliberately does not harvest, with the reason. A class
# belongs here only when *no* document it holds can become a fixture — never
# because grading it is inconvenient.
SKIP_CLASSES = {
    # Reads `../resources/validation/*.frees` itself; RESOURCE_DIRS harvests
    # those files directly and verbatim, so sweeping the class would only
    # re-derive them through the (lossier) string path.
    "ValidationSuiteTest.java": "documents come from RESOURCE_DIRS",
}

# Solve-site classifications the Wave-I inventory established as
# unrepresentable in the fixture format. The document text resolves, but the
# Java test hands the solver something the fixture cannot carry, so a
# default-settings golden would not be the answer the test asserts:
#   a-complex  complex mode (`new SolverSettings(..., true)`)
#   a-specs    `VariableSpec` guess/bounds overrides
#   a-settings any other non-default `SolverSettings`
#
# Since Wave Q these tags mean *the residue*: a site whose settings and specs
# both evaluate is carried by the `.request.json` sidecar and tagged
# `a-request` (plus an `a-request-*` sub-tag for the inventory) instead, and is
# staged. What still lands here is what the evaluator cannot turn into a
# request — imperative `new HashMap<>()` builders, and the `*-alien` sites
# where the matched `solve(` is not `EquationSystemSolver.solve` at all
# (`CasIdentity.solve`, the sparse `solve(double[])`). Both keep the *base*
# tag so the drop is unchanged; the sub-tag only records why.
SKIP_SITE_TAGS = ("a-complex", "a-specs", "a-settings")

# `SolverSettings.DEFAULTS` — (maxIterations, relativeResiduals,
# changeInVariables, elapsedTimeSeconds, complexMode). A settings argument
# equal to this carries no sidecar: it *is* the plain solve.
SETTINGS_DEFAULTS = (250, 1e-12, 1e-15, 3600.0, False)

# Java compile-time constants the evaluator resolves itself. `VariableSpec`
# bounds are spelled `Double.NEGATIVE_INFINITY` / `Double.POSITIVE_INFINITY`
# at nearly every site, and without these the whole a-specs class stays
# unresolvable. Deliberately a whitelist of *constants*: nothing here can run
# code, and an unlisted `Class.FIELD` still raises `crossfile` as before.
JAVA_CONSTANTS = {
    ("Double", "POSITIVE_INFINITY"): float("inf"),
    ("Double", "NEGATIVE_INFINITY"): float("-inf"),
    ("Double", "MAX_VALUE"): 1.7976931348623157e308,
    ("Double", "MIN_VALUE"): 4.9e-324,
    ("Integer", "MAX_VALUE"): 2147483647,
    ("Integer", "MIN_VALUE"): -2147483648,
    ("Math", "PI"): 3.141592653589793,
    ("Math", "E"): 2.718281828459045,
}

# Candidates permanently rejected by golden review: staging them again would
# re-mint a golden that cannot be frozen. Keyed by the name the sweep assigns,
# which is deterministic given the corpus it dedups against.
DROPPED = {
    # Ledger item 35: `~ignored~N` sink names are numbered by a JVM-batch-global
    # counter, so the oracle's own answer depends on what it dumped before.
    "multiout-mid-list-tilde-on-two-output-2": "ledger item 35 (~ignored~N)",
    "multiout-svd-discard-with-tilde-2": "ledger item 35 (~ignored~N)",
    "multiout-user-function-with-tilde-discard": "ledger item 35 (~ignored~N)",
    "multiout-user-function-with-tilde-discard-2": "ledger item 35 (~ignored~N)",
}

# A candidate whose first content line opens a markup tag is an expected
# *output* the test asserts on, not an input document — `VectorExportTest`'s
# `<svg …>` reaches the fragment guard with an `=` in it (an XML attribute) and
# would golden "the parser rejects markup", which is nobody's behaviour.
MARKUP_RE = re.compile(r"\s*<")

# A `DYNAMIC` output grid this dense is fine for the engine (the ceiling is
# `ode::problem::MAX_OUTPUT_SAMPLES`, 100 000) and ruinous for a *fixture*: the
# golden stores every cell, so `points = 10001` costs 1.6–3.1 MB and
# `points = 60001` costs **139 MB**, against 640 KB for the largest golden the
# corpus had. `dynamics_robustness::the_corpus_sample_counts_are_far_below_the
# _ceiling` also asserts the whole corpus stays a factor of ten under that
# ceiling, and the three Wave-J documents above the line broke it. 2000 keeps
# both properties and excludes nothing else: the densest document either side
# of the sweep declares 1201.
MAX_DECLARED_POINTS = 2000
POINTS_RE = re.compile(r"\bpoints\s*=\s*(\d+)", re.IGNORECASE)

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


def auto_prefix(basename):
    """The fixture prefix a swept class derives from its own name.

    `ComponentMoistAirTest.java` -> `component-moist-air`. Long, and
    deliberately so: a parity failure names its class without a lookup table,
    which is worth more than a short stem once the sweep covers 100+ classes.
    """
    return kebab(basename.removesuffix(".java").removesuffix("Test"))


def normalize(text):
    """A layout-insensitive key for "the same document".

    Frees variable names are case-insensitive and its statements are
    newline-separated, so folding case and collapsing runs of spaces *inside*
    a line — never the line structure, which is syntax — identifies candidates
    that differ from a corpus document only in indentation. Such a candidate
    adds no coverage and is skipped as a duplicate.
    """
    lines = (
        re.sub(r"[ \t]+", " ", line).strip()
        for line in text.strip().lower().split("\n")
    )
    return "\n".join(line for line in lines if line)


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
                elif (v.name, name) in JAVA_CONSTANTS:
                    v = JAVA_CONSTANTS[(v.name, name)]
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

        Each yield: (naming_pos, doc, in_throws, tables, request, tags)
          naming_pos — where to derive the fixture name from (the call site of
            the enclosing helper when parameter binding was needed);
          doc — the document text, or None when unresolvable;
          tables — the evaluated extra-defs tables (list, possibly empty);
          request — the evaluated solver request (dict) or None for defaults;
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
            request, tags = self._extra_arg_tags(callee, spans)
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
                        m.start(), (a0s, a0e), in_throws, tables, request, tags
                    )
                    continue
                if u.reason == "format" or uses_format:
                    tags.append("b-computed")
                elif u.reason == "crossfile":
                    tags.append("c-unresolved")
                yield m.start(), None, in_throws, tables, request, tags + [
                    f"unresolved-{u.reason}"
                ]
                continue
            if not isinstance(doc, str):
                continue
            if uses_format:
                tags.append("b-resolved")
            yield m.start(), doc, in_throws, tables, request, tags

    def _via_call_sites(self, call_pos, arg_span, in_throws, tables, request, tags):
        """Re-evaluate a param-blocked solve through its helper's call sites."""
        enc = self.enclosing_method(call_pos)
        if enc is None:
            yield call_pos, None, in_throws, tables, request, tags + [
                "unresolved-param"
            ]
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
                yield m.start(), None, in_throws, tables, request, tags + [
                    "b-helper",
                    f"unresolved-{u.reason}",
                ]
                continue
            if isinstance(doc, str):
                yield m.start(), doc, in_throws, tables, request, tags + ["b-helper"]

    def _extra_arg_tags(self, callee, spans):
        """Classify (and where possible *carry*) the extra solve arguments.

        Returns ``(request, tags)``. ``request`` is the `.request.json`
        sidecar body — ``{"stopCriteria": {...}, "variableInfo": [...]}``,
        either key present only when it differs from the engine default — or
        ``None`` when the site carries nothing beyond the defaults. ``tags``
        holds a SKIP_SITE_TAGS entry for every part that could *not* be
        represented; a site with a request and no skip tag is staged.

        Both halves must resolve or neither is used: half a request would
        golden an answer the Java test never asserted, which is the exact
        failure SKIP_SITE_TAGS was added to prevent.
        """
        tags = []
        request = {}
        texts = [self.code[s:e].strip() for s, e in spans]
        if callee in ("solve", "solveAll", "solvePermissive"):
            if len(spans) >= 2 and texts[1] != "SolverSettings.DEFAULTS":
                stop, bad = self._settings_of(spans[1])
                if bad:
                    tags.extend(bad)
                elif stop is not None:
                    request["stopCriteria"] = stop
                    tags.append(
                        "a-request-complex"
                        if stop["complexMode"]
                        else "a-request-settings"
                    )
            if len(spans) >= 3 and texts[2] != "Map.of()":
                info, bad = self._specs_of(spans[2])
                if bad:
                    tags.extend(bad)
                elif info:
                    request["variableInfo"] = info
                    tags.append("a-request-specs")
        elif callee == "check" and len(spans) >= 2:
            # `check(source, boolean complexMode, …)`: the dumper's oracle call
            # is `solve`, so a check site's second argument is a classification
            # only — it never becomes a request.
            if texts[1] not in ("false", "true"):
                tags.append("a-settings")
        if tags and any(t in SKIP_SITE_TAGS for t in tags):
            # Drop a half-built request with the site, and drop the
            # `a-request-*` sub-tags that described the half that did resolve.
            return None, [t for t in tags if not t.startswith("a-request")]
        if not request:
            return None, tags
        return request, tags + ["a-request"]

    def _settings_of(self, span):
        """A `SolverSettings` argument as a `stopCriteria` object.

        Returns ``(stop, bad_tags)``: ``stop`` is None when the argument is
        the engine default (nothing to carry) and a dict otherwise;
        ``bad_tags`` is non-empty when it cannot be represented.
        """
        try:
            v = self.eval_span(span[0], span[1], None)
        except Unresolved:
            return None, ["a-settings"]
        if not (
            isinstance(v, Ctor) and v.type_name.split(".")[-1] == "SolverSettings"
        ):
            # Not the engine's solve at all — `CasIdentity.solve(lhs, rhs, var)`
            # and the sparse `solve(double[])` both match CALL_RE.
            return None, ["a-settings", "a-settings-alien"]
        args = list(v.args)
        if len(args) == 4:
            args.append(False)
        if len(args) != 5:
            return None, ["a-settings"]
        nums, complex_mode = args[:4], args[4]
        if not isinstance(complex_mode, bool):
            return None, ["a-settings"]
        if any(isinstance(x, bool) or not isinstance(x, (int, float)) for x in nums):
            return None, ["a-settings"]
        values = (
            int(nums[0]),
            float(nums[1]),
            float(nums[2]),
            float(nums[3]),
            complex_mode,
        )
        if values == SETTINGS_DEFAULTS:
            return None, []
        return {
            "maxIterations": values[0],
            "relativeResiduals": values[1],
            "changeInVariables": values[2],
            "elapsedTimeSeconds": values[3],
            "complexMode": values[4],
        }, []

    def _specs_of(self, span):
        """A `Map<String, VariableSpec>` argument as `variableInfo` rows.

        Mirrors the Java record: the map key is what the solver looks a spec
        up by, and `VariableSpec`'s own constructor lowercases its name, so a
        key and a name that differ in anything but case make the site
        unrepresentable rather than silently one-of-the-two. An infinite bound
        is the *absence* of a bound (`Double.NEGATIVE_INFINITY` /
        `POSITIVE_INFINITY` are the record's own defaults), so it is omitted —
        which is exactly how the boundary's `VariableInfoDto` spells it.
        """
        try:
            v = self.eval_span(span[0], span[1], None)
        except Unresolved:
            return None, ["a-specs"]
        if not isinstance(v, list):
            return None, ["a-specs", "a-specs-alien"]
        rows = []
        for entry in v:
            if not (isinstance(entry, tuple) and len(entry) == 2):
                return None, ["a-specs", "a-specs-alien"]
            key, ctor = entry
            if not (
                isinstance(key, str)
                and isinstance(ctor, Ctor)
                and ctor.type_name.split(".")[-1] == "VariableSpec"
            ):
                return None, ["a-specs", "a-specs-alien"]
            args = list(ctor.args)
            if len(args) == 4:
                args.append(0.0)
            if len(args) != 5:
                return None, ["a-specs"]
            name, guess, lower, upper, unc = args
            if not isinstance(name, str):
                return None, ["a-specs"]
            if key.strip().lower() != name.strip().lower():
                return None, ["a-specs"]
            if any(
                isinstance(x, bool) or not isinstance(x, (int, float))
                for x in (guess, lower, upper, unc)
            ):
                return None, ["a-specs"]
            guess, lower, upper, unc = (
                float(guess),
                float(lower),
                float(upper),
                float(unc),
            )
            # A non-finite guess or uncertainty has no JSON spelling and no
            # sane meaning; the bounds do (see the docstring).
            if guess != guess or guess in (float("inf"), float("-inf")):
                return None, ["a-specs"]
            if unc != unc or unc in (float("inf"), float("-inf")):
                return None, ["a-specs"]
            if lower != lower or upper != upper:
                return None, ["a-specs"]
            row = {"name": key.strip().lower(), "guess": guess}
            if lower != float("-inf"):
                row["lower"] = lower
            if upper != float("inf"):
                row["upper"] = upper
            if unc:
                row["uncertainty"] = unc
            rows.append(row)
        return rows, []

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


def swept_classes():
    """(path, basename, prefix, mode) for every class the sweep harvests.

    Wave J: the whole test tree, minus SKIP_CLASSES. CLASSES supplies the
    prefix and extraction preference where it has one; every other class
    derives its prefix from its own name and prefers the solve-call path,
    which is the one that resolves constants, locals and helper inlining.
    """
    out = []
    for path in all_test_sources():
        base = os.path.basename(path)
        if base in SKIP_CLASSES:
            continue
        prefix, mode = CLASSES.get(base, (auto_prefix(base), "concat"))
        out.append((path, base, prefix, mode))
    return out


def sidecar_key(directory, stem, suffix):
    """A sidecar's canonical JSON text, or None when the document has none.

    Part of the duplicate key: the same document text solved under different
    request-level inputs is a *different* fixture (`x^2 = 4` from a guess of
    −1 and from a guess of +1 are the two roots), so the content alone cannot
    identify one.
    """
    path = os.path.join(directory, stem + suffix)
    if not os.path.exists(path):
        return None
    return json.dumps(json.load(open(path, encoding="utf-8")), sort_keys=True)


def existing_documents():
    """(trimmed content, tables json, request json) -> stem, the stem set, and
    the same map keyed by `normalize`d content (layout-insensitive
    duplicates)."""
    existing = {}
    near = {}
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
                tables = sidecar_key(d, stem, ".tables.json")
                request = sidecar_key(d, stem, ".request.json")
                existing[(content, tables, request)] = stem
                near.setdefault((normalize(content), tables, request), stem)
    return existing, stems, near


def line_of(src, pos):
    return src[:pos].count("\n") + 1


def inventory():
    paths = all_test_sources()
    registry = build_registry(paths)
    totals = {}
    examples = {}
    bearing = 0  # classes holding at least one harvestable document
    listed = 0  # ... of those, how many CLASSES names (the pre-Wave-J sweep)
    skipped_cls = 0  # ... and how many SKIP_CLASSES excludes
    print(f"{len(paths)} test classes under {JAVA_TEST_ROOT}\n")
    for p in paths:
        try:
            fh = FileHarvest(p, registry)
        except AssertionError as exc:
            print(f"  {os.path.basename(p)}: tokenizer failed ({exc})")
            continue
        rows = []
        for pos, doc, _thr, _tables, _request, tags in fh.solve_calls():
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
            base = os.path.basename(p)
            bearing += 1
            if base in SKIP_CLASSES:
                skipped_cls += 1
                mark = "!!"
            elif base in CLASSES:
                listed += 1
                mark = "  "
            else:
                mark = "+ "  # swept by name since Wave J
            flat = sorted({t for _, tags in rows for t in tags})
            print(
                f"{mark}{base:46} calls {len(rows):3} "
                f"(ok {n_res:3} / un {n_un:3})  blocks {len(blocks):3}  {flat}"
            )
    print("\ntotals:")
    for k in sorted(totals):
        print(f"  {k:22} {totals[k]:4}  {examples.get(k, '')}")
    print(
        f"\n{bearing} of {len(paths)} classes hold a harvestable document; "
        f"{listed} are named in CLASSES, {skipped_cls} in SKIP_CLASSES, "
        f"the remaining {bearing - listed - skipped_cls} are swept by name "
        f"(Wave J).\n"
        f"Sites dropped by SKIP_SITE_TAGS: "
        + ", ".join(f"{t} {totals.get(t, 0)}" for t in SKIP_SITE_TAGS)
        + f" (of which alien, i.e. not EquationSystemSolver.solve: "
        f"a-settings-alien {totals.get('a-settings-alien', 0)}, "
        f"a-specs-alien {totals.get('a-specs-alien', 0)})\n"
        f"Sites carried by a .request.json sidecar (Wave Q): "
        f"a-request {totals.get('a-request', 0)} — "
        + ", ".join(
            f"{t} {totals.get(t, 0)}"
            for t in ("a-request-complex", "a-request-settings", "a-request-specs")
        )
    )


def main():
    if "--inventory" in sys.argv:
        inventory()
        return
    out_corpus = DEFAULT_OUT
    if "--out" in sys.argv:
        out_corpus = sys.argv[sys.argv.index("--out") + 1]
    os.makedirs(out_corpus, exist_ok=True)
    existing, existing_stems, existing_near = existing_documents()

    registry = build_registry(all_test_sources())

    manifest = {}
    if os.path.exists(MANIFEST):
        manifest = json.load(open(MANIFEST, encoding="utf-8"))
    seen = {}  # (trimmed content, tables json) -> assigned name
    seen_near = {}  # the same, keyed layout-insensitively
    counts = {}
    name_counts = {}  # global: two classes may derive the same prefix
    skipped = {
        "fragment": 0,
        "template": 0,
        "markup": 0,
        "oversampled": 0,
        "dup_existing": 0,
        "dup_near": 0,
        "dup_harvest": 0,
        "unresolved": 0,
        "site_tag": 0,
        "dropped": 0,
    }

    def assign_name(base):
        n = name_counts.get(base, 0)
        while True:
            n += 1
            name = base if n == 1 else f"{base}-{n}"
            if name not in existing_stems and name not in seen.values():
                name_counts[base] = n
                return name

    def write_candidate(prefix, pos_name, doc, kind, in_throws, tables, request, cls):
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
        first = next(
            (
                line
                for line in trimmed.split("\n")
                if line.strip() and not line.strip().startswith("//")
            ),
            "",
        )
        if MARKUP_RE.match(first):
            skipped["markup"] += 1
            return
        if any(
            int(m.group(1)) > MAX_DECLARED_POINTS for m in POINTS_RE.finditer(trimmed)
        ):
            skipped["oversampled"] += 1
            return
        tables_key = json.dumps(tables, sort_keys=True) if tables else None
        request_key = json.dumps(request, sort_keys=True) if request else None
        key = (trimmed, tables_key, request_key)
        near_key = (normalize(trimmed), tables_key, request_key)
        if key in existing:
            skipped["dup_existing"] += 1
            return
        if key in seen:
            skipped["dup_harvest"] += 1
            return
        if near_key in existing_near:
            skipped["dup_near"] += 1
            return
        if near_key in seen_near:
            skipped["dup_near"] += 1
            return
        base = f"{prefix}-{kebab(pos_name)}"
        name = assign_name(base)
        if name in DROPPED:
            skipped["dropped"] += 1
            return
        seen[key] = name
        seen_near[near_key] = name
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
        if request:
            with open(os.path.join(out_corpus, name + ".request.json"), "w") as f:
                json.dump(request, f, indent=1)
                f.write("\n")
            entry["request"] = {
                "stop_criteria": sorted(request.get("stopCriteria", {})),
                "complex_mode": bool(
                    request.get("stopCriteria", {}).get("complexMode")
                ),
                "variable_info": [r["name"] for r in request.get("variableInfo", [])],
            }
        manifest[name] = entry

    for path, fname, prefix, mode in swept_classes():
        try:
            fh = FileHarvest(path, registry)
        except AssertionError as exc:
            print(f"  {fname}: tokenizer failed ({exc}) — skipped")
            continue

        candidates = []  # (pos, doc, kind, in_throws, tables, request)
        tabled_contents = set()
        for pos, doc, in_throws, tables, request, tags in fh.solve_calls():
            if doc is None:
                skipped["unresolved"] += 1
                continue
            if any(t in tags for t in SKIP_SITE_TAGS):
                skipped["site_tag"] += 1
                continue
            candidates.append((pos, doc, "solvecall", in_throws, tables, request))
            if tables:
                tabled_contents.add(doc.strip())
        for pos, doc in fh.text_blocks():
            # A text block whose content a table-carrying solve call also
            # produced is the same Java document — the tables belong to it;
            # staging a tables-less twin would golden an artifact error.
            #
            # A *request*-carrying twin is deliberately not suppressed here:
            # unlike an undefined Function Table, the same document under the
            # engine defaults is a perfectly well-defined solve, and the two
            # goldens are the point (the guess is what selects the root).
            if doc.strip() in tabled_contents:
                skipped["dup_harvest"] += 1
                continue
            candidates.append((pos, doc, "textblock", False, [], None))
        # extraction preference: preferred kind first, then position
        pref = "textblock" if mode == "text" else "solvecall"
        candidates.sort(key=lambda c: (0 if c[2] == pref else 1, c[0]))

        for pos, doc, kind, in_throws, tables, request in candidates:
            write_candidate(
                prefix,
                fh.enclosing(pos),
                doc,
                kind,
                in_throws,
                tables,
                request,
                fname,
            )

    for rel, prefix in RESOURCE_DIRS.items():
        d = os.path.join(RESOURCE_ROOT, rel)
        for f in sorted(os.listdir(d)):
            if not f.endswith(".frees"):
                continue
            doc = open(os.path.join(d, f), encoding="utf-8").read()
            write_candidate(
                prefix,
                f.removesuffix(".frees"),
                doc,
                "resource",
                False,
                [],
                None,
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
