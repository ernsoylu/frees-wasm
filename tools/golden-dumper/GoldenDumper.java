import com.frees.backend.ast.ProcDef;
import com.frees.backend.core.EquationSystemSolver;
import com.frees.backend.core.SolverSettings;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;

/**
 * Emits language-neutral golden fixtures from the Java frees engine.
 *
 * <p>This is the parity harness's oracle side (see {@code PLAN.md} §4). It runs
 * each {@code .frees} document in a corpus directory through the reference
 * {@link EquationSystemSolver} and records what the Java engine produced —
 * solved variables, display names, block count, or the error it raised. The
 * Rust engine replays the same corpus and is compared against these files.
 *
 * <p>Deliberately dependency-free beyond the engine itself: JSON is written by
 * hand so the tool needs no Jackson version on an already-crowded classpath.
 *
 * <p>It reads {@code ../frEES} but never writes to it.
 */
public final class GoldenDumper {

    /** Doubles are compared by tolerance, but emitted at full precision. */
    private static final int MAX_VARIABLES = 10_000;

    public static void main(String[] args) throws IOException {
        if (args.length < 2) {
            System.err.println("usage: GoldenDumper <corpus-dir> <output-dir>");
            System.exit(2);
        }
        Path corpus = Path.of(args[0]);
        Path outDir = Path.of(args[1]);
        Files.createDirectories(outDir);

        List<Path> documents;
        try (var stream = Files.list(corpus)) {
            documents = stream
                    .filter(p -> p.getFileName().toString().endsWith(".frees"))
                    .sorted(Comparator.comparing(Path::getFileName))
                    .toList();
        }

        if (documents.isEmpty()) {
            System.err.println("no .frees documents found in " + corpus.toAbsolutePath());
            System.exit(1);
        }

        int solved = 0;
        int failed = 0;
        for (Path doc : documents) {
            String name = doc.getFileName().toString().replaceFirst("\\.frees$", "");
            String source = Files.readString(doc, StandardCharsets.UTF_8);
            // A `<name>.tables.json` sidecar carries request-level Function
            // Tables (the GUI channel, `SolveDtos.functionTables`): they are
            // installed as extra defs exactly as `SolveController` does and
            // embedded in the fixture for `tests/parity.rs` to replay through
            // `solve_with_tables`.
            Path sidecar = corpus.resolve(name + ".tables.json");
            String tablesJson = Files.exists(sidecar)
                    ? Files.readString(sidecar, StandardCharsets.UTF_8)
                    : null;
            String json = dump(name, source, tablesJson);
            Files.writeString(outDir.resolve(name + ".json"), json, StandardCharsets.UTF_8);
            if (json.contains("\"error\": null")) {
                solved++;
            } else {
                failed++;
            }
            System.out.println("  " + name);
        }
        System.out.println("wrote " + documents.size() + " fixtures to " + outDir.toAbsolutePath()
                + " (" + solved + " solved, " + failed + " errored)");
    }

    /** Run one document through the engine and render the fixture JSON. */
    private static String dump(String name, String source, String tablesJson) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("  \"name\": ").append(quote(name)).append(",\n");
        sb.append("  \"source\": ").append(quote(source)).append(",\n");
        if (tablesJson != null) {
            // Verbatim: the value parity.rs replays is byte-identical to what
            // the harvester staged beside the corpus document.
            sb.append("  \"function_tables\": ").append(tablesJson.strip()).append(",\n");
        }
        sb.append("  \"expect\": {\n");

        try {
            Map<String, ProcDef> defs =
                    tablesJson == null ? Map.of() : tableDefs(tablesJson);
            EquationSystemSolver.Result result = new EquationSystemSolver()
                    .solve(source, SolverSettings.DEFAULTS, Map.of(), defs);

            Map<String, Double> vars = new TreeMap<>(result.variables());
            if (vars.size() > MAX_VARIABLES) {
                throw new IllegalStateException("document produced " + vars.size() + " variables");
            }
            sb.append("    \"variables\": {\n");
            List<String> lines = new ArrayList<>();
            for (Map.Entry<String, Double> e : vars.entrySet()) {
                lines.add("      " + quote(e.getKey()) + ": " + number(e.getValue()));
            }
            sb.append(String.join(",\n", lines));
            sb.append(lines.isEmpty() ? "" : "\n");
            sb.append("    },\n");

            Map<String, String> display = new TreeMap<>(result.displayNames());
            sb.append("    \"display_names\": {\n");
            List<String> dlines = new ArrayList<>();
            for (Map.Entry<String, String> e : display.entrySet()) {
                dlines.add("      " + quote(e.getKey()) + ": " + quote(e.getValue()));
            }
            sb.append(String.join(",\n", dlines));
            sb.append(dlines.isEmpty() ? "" : "\n");
            sb.append("    },\n");

            sb.append("    \"block_count\": ").append(result.blocks() == null ? 0 : result.blocks().size())
              .append(",\n");

            // ODE tables. A solved DYNAMIC block puts NOTHING in `variables` —
            // the trajectory lives here, in the first-class ODE Table. Without
            // this section a transient fixture compares only its algebraic
            // constants and passes vacuously, which is worse than no fixture.
            sb.append("    \"ode_tables\": [\n");
            List<String> tables = new ArrayList<>();
            if (result.odeTables() != null) {
                for (var table : result.odeTables()) {
                    StringBuilder t = new StringBuilder();
                    t.append("      {\n");
                    t.append("        \"name\": ").append(quote(table.name())).append(",\n");
                    t.append("        \"method\": ").append(quote(table.method())).append(",\n");
                    t.append("        \"stopped\": ").append(table.stopped()).append(",\n");
                    t.append("        \"end_time\": ").append(number(table.endTime())).append(",\n");
                    List<String> cols = new ArrayList<>();
                    for (String c : table.columns()) {
                        cols.add(quote(c));
                    }
                    t.append("        \"columns\": [").append(String.join(", ", cols)).append("],\n");
                    List<String> rows = new ArrayList<>();
                    for (List<Double> row : table.rows()) {
                        List<String> cells = new ArrayList<>();
                        for (Double cell : row) {
                            cells.add(number(cell));
                        }
                        rows.add("          [" + String.join(", ", cells) + "]");
                    }
                    t.append("        \"rows\": [\n").append(String.join(",\n", rows));
                    t.append(rows.isEmpty() ? "" : "\n").append("        ],\n");
                    List<String> hits = new ArrayList<>();
                    for (var e : table.events()) {
                        hits.add("          {\"name\": " + quote(e.name())
                                + ", \"time\": " + number(e.time()) + "}");
                    }
                    t.append("        \"events\": [\n").append(String.join(",\n", hits));
                    t.append(hits.isEmpty() ? "" : "\n").append("        ]\n");
                    t.append("      }");
                    tables.add(t.toString());
                }
            }
            sb.append(String.join(",\n", tables));
            sb.append(tables.isEmpty() ? "" : "\n");
            sb.append("    ],\n");

            sb.append("    \"error\": null\n");
        } catch (RuntimeException ex) {
            // A document that legitimately fails is just as valuable a fixture as
            // one that solves — the Rust engine must fail the same way.
            sb.append("    \"variables\": {},\n");
            sb.append("    \"display_names\": {},\n");
            sb.append("    \"block_count\": 0,\n");
            sb.append("    \"ode_tables\": [],\n");
            sb.append("    \"error\": {\n");
            sb.append("      \"type\": ").append(quote(ex.getClass().getSimpleName())).append(",\n");
            sb.append("      \"message\": ").append(quote(String.valueOf(ex.getMessage()))).append("\n");
            sb.append("    }\n");
        }

        sb.append("  },\n");
        sb.append("  \"oracle\": {\n");
        sb.append("    \"engine\": \"frEES backend/core (Java)\",\n");
        sb.append("    \"generated_by\": \"tools/golden-dumper\"\n");
        sb.append("  }\n");
        sb.append("}\n");
        return sb.toString();
    }

    /**
     * Full-precision double rendering. {@code Double.toString} round-trips
     * exactly, which is what a parity oracle needs; non-finite values become
     * JSON strings since JSON has no literal for them.
     */
    private static String number(Double d) {
        if (d == null) {
            return "null";
        }
        if (d.isNaN()) {
            return "\"NaN\"";
        }
        if (d.isInfinite()) {
            return d > 0 ? "\"Infinity\"" : "\"-Infinity\"";
        }
        return Double.toString(d);
    }

    private static String quote(String s) {
        if (s == null) {
            return "null";
        }
        StringBuilder out = new StringBuilder(s.length() + 16);
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                case '\b' -> out.append("\\b");
                case '\f' -> out.append("\\f");
                default -> {
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                }
            }
        }
        out.append('"');
        return out.toString();
    }

    /**
     * A `.tables.json` sidecar as solver extra defs — the shape
     * {@code SolveDtos.functionDefsOf} produces: one entry per table, keyed by
     * the trimmed lowercased name (later tables of a name win), the 5-argument
     * {@link ProcDef.FunctionTableDef} constructor (no declared units on this
     * channel). Curve samples are used exactly as written; the harvester
     * records what the Java test constructed, already ascending in x.
     */
    @SuppressWarnings("unchecked")
    private static Map<String, ProcDef> tableDefs(String json) {
        Map<String, ProcDef> defs = new LinkedHashMap<>();
        for (Object t : (List<Object>) Json.parse(json)) {
            Map<String, Object> table = (Map<String, Object>) t;
            String name = ((String) table.get("name")).trim().toLowerCase(Locale.ROOT);
            List<String> argNames = ((List<Object>) table.getOrDefault("arg_names", List.of()))
                    .stream().map(o -> (String) o).toList();
            List<ProcDef.Curve> curves = new ArrayList<>();
            for (Object c : (List<Object>) table.get("curves")) {
                Map<String, Object> curve = (Map<String, Object>) c;
                curves.add(new ProcDef.Curve((Double) curve.get("param"),
                        doubles(curve.get("xs")), doubles(curve.get("ys"))));
            }
            defs.put(name, new ProcDef.FunctionTableDef(name, argNames,
                    Boolean.TRUE.equals(table.get("x_log")),
                    Boolean.TRUE.equals(table.get("y_log")), curves));
        }
        return defs;
    }

    @SuppressWarnings("unchecked")
    private static double[] doubles(Object o) {
        List<Object> l = (List<Object>) o;
        double[] out = new double[l.size()];
        for (int i = 0; i < out.length; i++) {
            out[i] = (Double) l.get(i);
        }
        return out;
    }

    /**
     * Minimal recursive-descent JSON reader for the sidecar files. Objects
     * become {@link LinkedHashMap}, arrays {@link ArrayList}, numbers
     * {@link Double}; this tool stays dependency-free (see the class doc), and
     * the writer side is the harvester, so the input shape is known.
     */
    private static final class Json {
        private final String s;
        private int i;

        private Json(String s) {
            this.s = s;
        }

        static Object parse(String s) {
            Json j = new Json(s);
            Object v = j.value();
            j.ws();
            if (j.i < j.s.length()) {
                throw new IllegalArgumentException("trailing JSON at offset " + j.i);
            }
            return v;
        }

        private void ws() {
            while (i < s.length() && Character.isWhitespace(s.charAt(i))) {
                i++;
            }
        }

        private void expect(char c) {
            ws();
            if (i >= s.length() || s.charAt(i) != c) {
                throw new IllegalArgumentException("expected '" + c + "' at offset " + i);
            }
            i++;
        }

        private Object value() {
            ws();
            char c = s.charAt(i);
            if (c == '{') {
                i++;
                Map<String, Object> m = new LinkedHashMap<>();
                ws();
                if (s.charAt(i) == '}') {
                    i++;
                    return m;
                }
                while (true) {
                    ws();
                    String k = (String) value();
                    expect(':');
                    m.put(k, value());
                    ws();
                    if (s.charAt(i) == ',') {
                        i++;
                        continue;
                    }
                    expect('}');
                    return m;
                }
            }
            if (c == '[') {
                i++;
                List<Object> l = new ArrayList<>();
                ws();
                if (s.charAt(i) == ']') {
                    i++;
                    return l;
                }
                while (true) {
                    l.add(value());
                    ws();
                    if (s.charAt(i) == ',') {
                        i++;
                        continue;
                    }
                    expect(']');
                    return l;
                }
            }
            if (c == '"') {
                return string();
            }
            if (s.startsWith("true", i)) {
                i += 4;
                return Boolean.TRUE;
            }
            if (s.startsWith("false", i)) {
                i += 5;
                return Boolean.FALSE;
            }
            if (s.startsWith("null", i)) {
                i += 4;
                return null;
            }
            int start = i;
            while (i < s.length() && "+-.eE0123456789".indexOf(s.charAt(i)) >= 0) {
                i++;
            }
            return Double.parseDouble(s.substring(start, i));
        }

        private String string() {
            StringBuilder b = new StringBuilder();
            i++; // opening quote
            while (true) {
                char c = s.charAt(i++);
                if (c == '"') {
                    return b.toString();
                }
                if (c != '\\') {
                    b.append(c);
                    continue;
                }
                char e = s.charAt(i++);
                switch (e) {
                    case '"' -> b.append('"');
                    case '\\' -> b.append('\\');
                    case '/' -> b.append('/');
                    case 'n' -> b.append('\n');
                    case 't' -> b.append('\t');
                    case 'r' -> b.append('\r');
                    case 'b' -> b.append('\b');
                    case 'f' -> b.append('\f');
                    case 'u' -> {
                        b.append((char) Integer.parseInt(s.substring(i, i + 4), 16));
                        i += 4;
                    }
                    default -> throw new IllegalArgumentException("bad escape \\" + e);
                }
            }
        }
    }

    private GoldenDumper() {
    }
}
