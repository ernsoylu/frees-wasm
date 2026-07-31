import com.frees.backend.core.dae.DaeJacobian;
import com.frees.backend.core.dae.DaeResidual;
import com.frees.backend.core.dae.DaeRootFn;
import com.frees.backend.core.dae.IdaDaeSolver;
import com.frees.backend.core.dae.SparseSteadyKlu;
import com.frees.backend.core.dae.SundialsIda;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Ground-truth probe for the Rust DAE port (`crates/frees-core/src/dae/`).
 *
 * <p>The frees DAE stack reaches SUNDIALS IDA through JNA, which a wasm build
 * cannot do, so `dae/solver.rs` re-implements IDA's variable-order
 * variable-step BDF in Rust. "The numerical contract is what must match, not
 * the library" — this program produces that contract: it drives the REAL
 * `IdaDaeSolver` / `DaeJacobian` / `SparseSteadyKlu` over a set of analytic DAE
 * problems and dumps the trajectories, Jacobians and sparse solves as JSON.
 * The Rust tests replay the same problems and compare.
 *
 * <p>This exists because the document-level oracle (tools/golden-dumper) cannot
 * reach the IDA path without the DYNAMIC grammar; these are API-level fixtures.
 *
 * <p>Requires libsundials_ida >= 6 (SUNContext API). Prints
 * `"available": false` and exits 0 when it is absent, exactly as the Java
 * engine degrades.
 */
public final class DaeProbe {

    private DaeProbe() {}

    public static void main(String[] args) throws IOException {
        Path out = Path.of(args.length > 0 ? args[0] : "dae-oracle.json");
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("  \"oracle\": \"SUNDIALS IDA via frEES core IdaDaeSolver (JNA)\",\n");
        sb.append("  \"available\": ").append(SundialsIda.isAvailable()).append(",\n");
        sb.append("  \"sparse_available\": ").append(SundialsIda.sparseAvailable()).append(",\n");

        sb.append("  \"jacobian\": ").append(jacobianCases()).append(",\n");
        sb.append("  \"sparse_solve\": ").append(sparseSolveCase()).append(",\n");
        sb.append("  \"trajectories\": [\n");
        List<String> traj = new ArrayList<>();
        if (SundialsIda.isAvailable()) {
            traj.add(newtonCooling());
            traj.add(index1Linear());
            traj.add(robertson());
            traj.add(robertsonRoots());
            traj.add(heatChain());
            traj.add(stiffAlgebraicLoop());
            traj.add(monotoneAlgebraicLoop());
            traj.add(newtonCoolingTight());
        }
        sb.append(String.join(",\n", traj));
        sb.append(traj.isEmpty() ? "" : "\n");
        sb.append("  ]\n");
        sb.append("}\n");
        Files.writeString(out, sb.toString(), StandardCharsets.UTF_8);
        System.out.println("wrote " + out.toAbsolutePath());
    }

    // ── Jacobian cases (pure Java; no native library needed) ──────────────────

    /**
     * A residual whose combined Jacobian J = dF/dy + cj*dF/dy' is known in
     * closed form, plus the greedy column colouring of a banded pattern.
     */
    private static String jacobianCases() {
        // F0 = yp0 + 3*y0*y1 - 7
        // F1 = y1*y1 - y0 - 2*yp1
        // F2 = y2 - y0*y1
        DaeResidual res = (t, y, yp, r) -> {
            r[0] = yp[0] + 3.0 * y[0] * y[1] - 7.0;
            r[1] = y[1] * y[1] - y[0] - 2.0 * yp[1];
            r[2] = y[2] - y[0] * y[1] + t;
        };
        double[] y = {1.5, -0.75, 2.25};
        double[] yp = {0.5, 0.25, -1.0};
        double t = 0.3;
        double cj = 4.0;
        double[][] dense = DaeJacobian.dense(res, t, cj, y, yp);

        int[][] rowsPattern = {{0, 1}, {0, 1}, {0, 1, 2}};
        int[] color = DaeJacobian.colorColumns(rowsPattern, 3);
        // per-column row lists (transpose of rowsPattern)
        int[][] colRows = {{0, 1, 2}, {0, 1, 2}, {2}};
        double[][] colored = DaeJacobian.denseColored(res, t, cj, y, yp, colRows, color);

        // Banded 8x8 tridiagonal pattern -> the colouring the S2 path relies on.
        int n = 8;
        int[][] band = new int[n][];
        for (int i = 0; i < n; i++) {
            List<Integer> cols = new ArrayList<>();
            if (i > 0) {
                cols.add(i - 1);
            }
            cols.add(i);
            if (i < n - 1) {
                cols.add(i + 1);
            }
            band[i] = cols.stream().mapToInt(Integer::intValue).toArray();
        }
        int[] bandColor = DaeJacobian.colorColumns(band, n);

        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("    \"t\": ").append(num(t)).append(", \"cj\": ").append(num(cj)).append(",\n");
        sb.append("    \"y\": ").append(vec(y)).append(", \"yp\": ").append(vec(yp)).append(",\n");
        sb.append("    \"dense\": ").append(mat(dense)).append(",\n");
        sb.append("    \"colors_3\": ").append(ivec(color)).append(",\n");
        sb.append("    \"colored\": ").append(mat(colored)).append(",\n");
        sb.append("    \"colors_tridiag8\": ").append(ivec(bandColor)).append("\n");
        sb.append("  }");
        return sb.toString();
    }

    /** A fixed CSC pattern solved by KLU (or reported unavailable). */
    private static String sparseSolveCase() {
        // A = [[4,1,0],[1,3,1],[0,1,2]] in CSC: col0 rows{0,1}, col1 rows{0,1,2}, col2 rows{1,2}
        List<List<Integer>> pattern = List.of(List.of(0, 1), List.of(0, 1, 2), List.of(1, 2));
        double[] values = {4.0, 1.0, 1.0, 3.0, 1.0, 1.0, 2.0};
        double[] b = {1.0, 2.0, 3.0};
        double[] x = null;
        int nnz = -1;
        try (SparseSteadyKlu klu = SparseSteadyKlu.create(pattern)) {
            if (klu != null) {
                nnz = klu.nonzeros();
                x = klu.solve(values, b);
            }
        } catch (RuntimeException e) {
            x = null;
        }
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("    \"available\": ").append(SparseSteadyKlu.available()).append(",\n");
        sb.append("    \"nnz\": ").append(nnz).append(",\n");
        sb.append("    \"values\": ").append(vec(values)).append(",\n");
        sb.append("    \"b\": ").append(vec(b)).append(",\n");
        sb.append("    \"x\": ").append(x == null ? "null" : vec(x)).append("\n");
        sb.append("  }");
        return sb.toString();
    }

    // ── Trajectory cases (need libsundials_ida) ───────────────────────────────

    /** dT/dt = k (Tinf - T): the fixture the harness already has an ode45 golden for. */
    private static String newtonCooling() {
        DaeResidual res = (t, y, yp, r) -> r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        double[] times = {0.0, 20.0, 40.0, 60.0};
        return run("newton_cooling", 1, res, new double[] {1.0},
                new double[] {95.0}, new double[] {0.0}, times, 1e-6, 1e-8, null, 0);
    }

    /** Semi-explicit index-1: yp0 + y0 - y1 = 0, y1 - 2*y0 = 0 -> y0 = exp(t). */
    private static String index1Linear() {
        DaeResidual res = (t, y, yp, r) -> {
            r[0] = yp[0] + y[0] - y[1];
            r[1] = y[1] - 2.0 * y[0];
        };
        double[] times = {0.0, 0.5, 1.0, 1.5, 2.0};
        return run("index1_linear", 2, res, new double[] {1.0, 0.0},
                new double[] {1.0, 0.0}, new double[] {0.0, 0.0}, times, 1e-8, 1e-10, null, 0);
    }

    /** Robertson's stiff chemical kinetics DAE (SUNDIALS' own idaRoberts example). */
    private static String robertson() {
        DaeResidual res = DaeProbe::robertsonResidual;
        double[] times = {0.0, 0.4, 4.0, 40.0, 400.0, 4000.0, 40000.0};
        return run("robertson", 3, res, new double[] {1.0, 1.0, 0.0},
                new double[] {1.0, 0.0, 0.0}, new double[] {-0.04, 0.04, 0.0},
                times, 1e-8, 1e-10, null, 0);
    }

    private static void robertsonResidual(double t, double[] y, double[] yp, double[] r) {
        r[0] = yp[0] + 0.04 * y[0] - 1.0e4 * y[1] * y[2];
        r[1] = yp[1] - 0.04 * y[0] + 1.0e4 * y[1] * y[2] + 3.0e7 * y[1] * y[1];
        r[2] = y[0] + y[1] + y[2] - 1.0;
    }

    /** Robertson plus the two switching functions the IDA example roots on. */
    private static String robertsonRoots() {
        DaeResidual res = DaeProbe::robertsonResidual;
        DaeRootFn root = (t, y, yp, g) -> {
            g[0] = y[0] - 0.8;
            g[1] = y[2] - 1.0e-4;
        };
        double[] times = {0.0, 0.4, 4.0, 40.0, 400.0};
        return run("robertson_roots", 3, res, new double[] {1.0, 1.0, 0.0},
                new double[] {1.0, 0.0, 0.0}, new double[] {-0.04, 0.04, 0.0},
                times, 1e-8, 1e-10, root, 2);
    }

    /**
     * A 1-D heat chain in C-R-C form: 16 capacitive node temperatures (states)
     * interleaved with 15 algebraic conductive fluxes — n = 31, so the assembly
     * is above the sparse threshold and banded, which is what the colouring and
     * CSC path are for.
     */
    private static String heatChain() {
        int nodes = 16;
        int fluxes = nodes - 1;
        int n = nodes + fluxes;
        double cap = 500.0;
        double cond = 12.0;
        DaeResidual res = (t, y, yp, r) -> {
            for (int i = 0; i < nodes; i++) {
                double in = i > 0 ? y[nodes + i - 1] : 0.0;
                double outq = i < nodes - 1 ? y[nodes + i] : 0.0;
                r[i] = cap * yp[i] - (in - outq);
            }
            for (int k = 0; k < fluxes; k++) {
                r[nodes + k] = y[nodes + k] - cond * (y[k] - y[k + 1]);
            }
        };
        double[] y0 = new double[n];
        double[] id = new double[n];
        for (int i = 0; i < nodes; i++) {
            y0[i] = i == 0 ? 400.0 : 300.0;
            id[i] = 1.0;
        }
        double[] times = {0.0, 50.0, 200.0, 1000.0};
        return run("heat_chain", n, res, id, y0, new double[n], times, 1e-8, 1e-10, null, 0);
    }

    /**
     * Newton cooling again at a tolerance tight enough that the local
     * truncation error no longer dominates. At rtol 1e-6 two correct BDF codes
     * legitimately differ by ~1e-6 (both ride the tolerance limit); this run
     * exists so the Rust port can be graded on the trajectory rather than on
     * the tolerance.
     */
    private static String newtonCoolingTight() {
        DaeResidual res = (t, y, yp, r) -> r[0] = yp[0] - 0.05 * (20.0 - y[0]);
        double[] times = {0.0, 20.0, 40.0, 60.0};
        return run("newton_cooling_tight", 1, res, new double[] {1.0},
                new double[] {95.0}, new double[] {0.0}, times, 1e-10, 1e-12, null, 0);
    }

    /**
     * The same coupled loop as {@link #stiffAlgebraicLoop} with the quadratic
     * constraint replaced by a strictly monotone cubic, so the algebraic
     * Jacobian never passes through zero. This is the well-posed index-1
     * acceptance case; the quadratic one below is kept because its turning
     * point is where the two integrators are ALLOWED to diverge.
     */
    private static String monotoneAlgebraicLoop() {
        DaeResidual res = (t, y, yp, r) -> {
            r[0] = yp[0] + y[1] * y[2];
            r[1] = y[1] - Math.exp(-y[0]) - 0.1 * y[2];
            r[2] = y[2] + 0.5 * y[2] * y[2] * y[2] + y[1] - 1.25 - 0.5 * Math.sin(t);
        };
        double[] times = {0.0, 0.25, 1.0, 3.0, 8.0};
        return run("monotone_algebraic_loop", 3, res, new double[] {1.0, 0.0, 0.0},
                new double[] {0.5, 0.6, 0.8}, new double[] {0.0, 0.0, 0.0},
                times, 1e-9, 1e-11, null, 0);
    }

    /**
     * A stiff algebraic loop: one state driving two coupled algebraic
     * auxiliaries whose Jacobian block is dense. Exercises the Newton inside
     * IDA rather than the BDF history.
     *
     * <p><b>Not an acceptance case.</b> Its constraint {@code y2² + y1 = …}
     * has a turning point where {@code ∂F/∂y2 = 2 y2} passes through zero and
     * the two solution branches merge — the DAE is not index-1 there. Which
     * branch a code emerges on is a numerical accident, so this run is recorded
     * only to document what the oracle does.
     */
    private static String stiffAlgebraicLoop() {
        DaeResidual res = (t, y, yp, r) -> {
            r[0] = yp[0] + y[1] * y[2];
            r[1] = y[1] - Math.exp(-y[0]) - 0.1 * y[2];
            r[2] = y[2] * y[2] + y[1] - 1.25 - 0.5 * Math.sin(t);
        };
        double[] times = {0.0, 0.25, 1.0, 3.0, 8.0};
        return run("stiff_algebraic_loop", 3, res, new double[] {1.0, 0.0, 0.0},
                new double[] {0.5, 0.6, 0.8}, new double[] {0.0, 0.0, 0.0},
                times, 1e-9, 1e-11, null, 0);
    }

    // ── driver ────────────────────────────────────────────────────────────────

    /**
     * Mirrors {@code DynamicSolver.solveWithIda}: tolerances, variable id,
     * roots, IDACalcIC(IDA_YA_YDP_INIT) at t0 + span*1e-3, then a normal-mode
     * step to each sample time. Root returns are recorded and integration
     * continues (the non-stop event action).
     */
    private static String run(String name, int n, DaeResidual res, double[] id,
                              double[] y0, double[] yp0, double[] times,
                              double rtol, double atol, DaeRootFn rootFn, int nroots) {
        StringBuilder sb = new StringBuilder();
        sb.append("    {\n");
        sb.append("      \"name\": \"").append(name).append("\",\n");
        sb.append("      \"n\": ").append(n).append(",\n");
        sb.append("      \"rtol\": ").append(num(rtol)).append(", \"atol\": ").append(num(atol)).append(",\n");
        sb.append("      \"y0\": ").append(vec(y0)).append(", \"yp0\": ").append(vec(yp0)).append(",\n");
        sb.append("      \"id\": ").append(vec(id)).append(",\n");
        List<String> rows = new ArrayList<>();
        List<String> roots = new ArrayList<>();
        String ic = "null";
        String error = "null";
        try (IdaDaeSolver s = new IdaDaeSolver(n, res)) {
            s.setTolerances(rtol, atol);
            s.setVariableId(id);
            if (rootFn != null && nroots > 0) {
                s.setRoots(nroots, rootFn);
            }
            s.init(times[0], y0, yp0);
            double span = times[times.length - 1] - times[0];
            try {
                s.calcConsistentIc(SundialsIda.IDA_YA_YDP_INIT, times[0] + span * 1e-3);
            } catch (IllegalStateException icFailed) {
                s.reinit(times[0], y0, yp0);
            }
            ic = "{\"y\": " + vec(s.currentState()) + ", \"yp\": " + vec(s.currentDerivative()) + "}";
            rows.add("        {\"t\": " + num(times[0]) + ", \"y\": " + vec(s.currentState())
                    + ", \"yp\": " + vec(s.currentDerivative()) + "}");
            for (int i = 1; i < times.length; i++) {
                IdaDaeSolver.Step step = s.step(times[i]);
                while (step.rootReturn()) {
                    roots.add("        {\"t\": " + num(step.t()) + ", \"found\": " + ivec(step.rootsFound())
                            + ", \"y\": " + vec(step.y()) + "}");
                    step = s.step(times[i]);
                }
                rows.add("        {\"t\": " + num(step.t()) + ", \"y\": " + vec(step.y())
                        + ", \"yp\": " + vec(step.yp()) + "}");
            }
        } catch (RuntimeException ex) {
            error = "\"" + ex.getClass().getSimpleName() + ": " + String.valueOf(ex.getMessage()) + "\"";
        }
        sb.append("      \"consistent_ic\": ").append(ic).append(",\n");
        sb.append("      \"rows\": [\n").append(String.join(",\n", rows));
        sb.append(rows.isEmpty() ? "" : "\n").append("      ],\n");
        sb.append("      \"roots\": [\n").append(String.join(",\n", roots));
        sb.append(roots.isEmpty() ? "" : "\n").append("      ],\n");
        sb.append("      \"error\": ").append(error).append("\n");
        sb.append("    }");
        return sb.toString();
    }

    // ── JSON helpers ──────────────────────────────────────────────────────────

    private static String num(double d) {
        if (Double.isNaN(d)) {
            return "\"NaN\"";
        }
        if (Double.isInfinite(d)) {
            return d > 0 ? "\"Infinity\"" : "\"-Infinity\"";
        }
        return Double.toString(d);
    }

    private static String vec(double[] v) {
        List<String> parts = new ArrayList<>();
        for (double d : v) {
            parts.add(num(d));
        }
        return "[" + String.join(", ", parts) + "]";
    }

    private static String ivec(int[] v) {
        List<String> parts = new ArrayList<>();
        for (int d : v) {
            parts.add(Integer.toString(d));
        }
        return "[" + String.join(", ", parts) + "]";
    }

    private static String mat(double[][] m) {
        List<String> rows = new ArrayList<>();
        for (double[] row : m) {
            rows.add(vec(row));
        }
        return "[" + String.join(", ", rows) + "]";
    }
}
