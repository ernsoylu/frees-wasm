import com.frees.backend.core.EquationSystemSolver;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
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
            String json = dump(name, source);
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
    private static String dump(String name, String source) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("  \"name\": ").append(quote(name)).append(",\n");
        sb.append("  \"source\": ").append(quote(source)).append(",\n");
        sb.append("  \"expect\": {\n");

        try {
            EquationSystemSolver.Result result = new EquationSystemSolver().solve(source);

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

    private GoldenDumper() {
    }
}
