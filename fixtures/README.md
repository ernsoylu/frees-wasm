# Parity fixtures

The correctness backbone of the port (`PLAN.md` §4). The Java engine has 1,237
JUnit tests; hand-translating 24,359 lines of test code would be the project's
biggest mistake. Instead both engines run the **same corpus** and are compared.

```
fixtures/corpus/*.frees   the documents (hand-authored + harvested)
fixtures/golden/*.json    what the Java engine produced for each
tools/golden-dumper/      the Java side that generates fixtures/golden
```

## Regenerating

```bash
tools/golden-dumper/run.sh                    # corpus -> golden
tools/golden-dumper/run.sh <corpus> <out>     # explicit paths
```

`classpath.sh` locates the newest `core-*.jar` under
`../frEES/backend/core/build/libs/` and assembles the dependency classpath from
the Gradle cache. It needs no Gradle run and **never writes to `../frEES`**.

Two classpath hazards it handles, both found the hard way:

* The cache holds `antlr4-runtime` 4.7.2 **and** 4.13.2. If the old one wins,
  every parse dies in class-initialisation with a version mismatch. Pinned jars
  go first and 4.7.x is filtered out.
* Multiple SLF4J providers are present; Logback is excluded so the binding is
  unambiguous.

Override with `FREES_HOME`, `FREES_CORE_JAR`, or `GRADLE_CACHE`.

## Fixture format

```json
{
  "name": "canonical",
  "source": "x^2 + y^3 = 77\nx/y = 1.23456\n",
  "expect": {
    "variables":     { "x": 4.694012391660914, "y": 3.802174371161316 },
    "display_names": { "x": "x", "y": "y" },
    "block_count": 1,
    "error": null
  },
  "oracle": { "engine": "frEES backend/core (Java)", "generated_by": "tools/golden-dumper" }
}
```

A document that **fails** is as valuable as one that solves — the Rust engine
must fail the same way:

```json
"error": { "type": "SolverException", "message": "There are 2 equations and 1 variables. …" }
```

## Comparison policy

Compare by **relative tolerance, never bit-equality**. WASM `f64` arithmetic is
IEEE-754 deterministic, but transcendentals (`exp`, `ln`, `pow`, `sin`) are
unspecified and differ between the JVM and Rust's libm.

| Field | Rule |
|---|---|
| `variables` | relative tolerance `1e-9`; absolute `1e-12` near zero |
| `display_names` | exact |
| `block_count` | exact — a different blocking is a real divergence |
| `error` | `type` exact; `message` **not** compared verbatim (see below) |

Error *messages* are not compared literally. The Java engine emits long,
domain-specific guidance ("A common cause: an element chain with no constitutive
law for that quantity…"). Matching that prose is not the goal; matching the
*classification* is. Assert the error type and that the message names the same
offending variables.

## Ground truth this corpus already pinned down

Behaviours that would otherwise have been guesses, now settled by running the
oracle rather than reading the source:

| Fixture | Establishes |
|---|---|
| `arithmetic` | `2^3^2 = 512` — exponentiation is **right**-associative. `-2^2 = -4` — unary minus binds **looser** than `^`. `-` and `/` are left-associative. |
| `units_negative_celsius` | `-10 [C]` = **263.15 K**, not −283.15. The unary sign folds into the literal *before* unit conversion (`AstBuilder.bareUnitLiteral`). |
| `units_pressure` | `140 [kPa]` = `140000.0` — SI conversion happens at **parse** time. |
| `units_temperature` | `25 [C]` = 298.15, `32 [F]` = 273.15 — additive offsets, `F` factor 5/9. |
| `intrinsics` | `sin(0)=0`, `cos(0)=1` — trig takes **radians**. |
| `constants` | `pi# = 3.141592653589793`, `R# = 8.314462618`, `g# = 9.80665`. |
| `case_insensitive` | `Tin`/`TIN`/`tin` are one variable; the result key keeps the **first-seen** spelling, and `display_names` maps lowercase → that spelling. |
| `sequential` | Blocks solve in **dependency** order, not source order (3 blocks). |
| `overdetermined` / `underdetermined` | Both are `SolverException`, and the message names the specific redundant relation / free quantity. |
| `empty` | A comment-only document is `SolverException: "No equations to solve."`, not success-with-nothing. |

## Growing the corpus

Current corpus is a Phase-1 seed (scalar equations, units, precedence, blocking,
error paths). Extend it from, in rough order of value:

1. `../frEES/frontend/src/examples.ts` (51 KB of whole example documents)
2. `../frEES/frontend/src/docs/*.md` (documented snippets)
3. the `*ExamplesTest.java` classes (`CycleExamplesTest`, `HvacExamplesTest`,
   `SystemDesignExamplesTest`) — already whole-document round-trips
4. `../frEES/backend/core/src/main/resources/components/*.frees` — all 295
   library components, once the component expander lands (Phase 6)

Add the document to `corpus/`, rerun `run.sh`, and commit both the source and
the generated golden file. **Review the generated fixture before committing** —
it encodes whatever the Java engine does, including any bug.
