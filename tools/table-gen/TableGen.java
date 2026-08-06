package com.frees.backend.props;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;
import java.util.function.DoubleBinaryOperator;

/**
 * Generates phase-split {@code (P,h)} property tables from native CoolProp and
 * measures the error they introduce — the D1 deliverable
 * ({@code docs/decisions/0001-property-backend.md}).
 *
 * <p>Sibling of {@code tools/golden-dumper}: same {@code classpath.sh}, same
 * discipline of adding no dependency the reference engine does not already
 * carry. JSON is written by hand; the only jar it touches beyond the core
 * engine is JNA, which the engine's own CoolProp binding already requires.
 *
 * <h2>Why this class lives in {@code com.frees.backend.props}</h2>
 *
 * Not for convenience — for provability. The reference engine already contains
 * the exact architecture this port needs ({@link PhPropertyTable},
 * {@link SaturationSplitTable}, {@code PhTableRegistry}), but
 * {@code SaturationSplitTable} is package-private and hard-codes its
 * resolution. Declaring this tool in the same package lets it
 *
 * <ul>
 *   <li>build its grids with the <b>real</b> {@link PhPropertyTable}, so the
 *       interpolation error it reports is the error of the code the Rust port
 *       transcribes — not of a lookalike written for the measurement; and</li>
 *   <li>cross-check its (necessarily re-stated, because it must be
 *       resolution-parametric) split geometry against the real
 *       {@link SaturationSplitTable} at the reference resolution, so a
 *       transcription slip shows up as a hard failure instead of a quietly
 *       wrong table.</li>
 * </ul>
 *
 * <h2>What it emits</h2>
 *
 * One {@code <fluid>.phtab} per fluid in the binary format documented in
 * {@code README.md}, plus {@code MANIFEST.json} (grid bounds, resolution,
 * CoolProp version, SHA-256, byte size) and {@code ERROR-REPORT.json} (the
 * measured tabulation error, per property and per query form).
 *
 * <p>It reads {@code ../frEES} but never writes to it.
 */
public final class TableGen {

    /** Tabulated outputs, in serialized plane order. Mirrors {@code PhTableRegistry.TABLE_OUTPUTS}. */
    static final String[] OUTPUTS = {"T", "Dmass", "Smass"};

    /** Liquid piece needs at least this much saturated-to-cold headroom (from {@code SaturationSplitTable}). */
    private static final double MIN_LIQUID_DEPTH = 2.0e4;

    /** Format identifier written at byte 0 of every {@code .phtab}. */
    private static final byte[] MAGIC = "FRPHTAB1".getBytes(StandardCharsets.US_ASCII);
    private static final int FORMAT_VERSION = 1;

    /** Liquid-piece second coordinate. */
    enum LiquidCoord {
        /** {@code y = h_f(P) - h}, capped by one depth valid at every pressure (the reference geometry). */
        ABSOLUTE,
        /** {@code y = (h_f(P) - h) / (h_f(P) - h_cold(P))}, so the piece follows the whole liquid sliver. */
        NORMALIZED
    }

    private TableGen() {
    }

    // ---------------------------------------------------------------- main

    public static void main(String[] args) throws IOException {
        if (args.length < 1) {
            System.err.println("""
                    usage: TableGen <out-dir> [options]
                      --fluids A,B      fluids to tabulate           (default Water,R134a)
                      --nsat N          saturation-line samples      (default 256)
                      --np N            pressure nodes per piece     (default 96)
                      --ndh N           depth nodes per piece        (default 48)
                      --liquid MODE     absolute | normalized        (default normalized)
                      --f32             store the payload as f32     (default f64)
                      --samples N       error-measurement samples    (default 4000)
                      --seed N          sample seed                  (default 20260730)
                      --sweep           measure a resolution ladder, write no tables
                      --no-verify       skip the error measurement
                      --no-cross-check  skip the proof against SaturationSplitTable""");
            System.exit(2);
        }
        Path outDir = Path.of(args[0]);

        List<String> fluids = List.of("Water", "R134a");
        int nSat = 256;
        int nP = 96;
        int nDh = 48;
        LiquidCoord liquid = LiquidCoord.NORMALIZED;
        boolean f32 = false;
        int samples = 4000;
        long seed = 20260730L;
        boolean sweep = false;
        boolean verify = true;
        boolean cross = true;

        for (int i = 1; i < args.length; i++) {
            switch (args[i]) {
                case "--fluids" -> fluids = List.of(args[++i].split(","));
                case "--nsat" -> nSat = Integer.parseInt(args[++i]);
                case "--np" -> nP = Integer.parseInt(args[++i]);
                case "--ndh" -> nDh = Integer.parseInt(args[++i]);
                case "--liquid" -> liquid = LiquidCoord.valueOf(args[++i].toUpperCase(Locale.ROOT));
                case "--f32" -> f32 = true;
                case "--samples" -> samples = Integer.parseInt(args[++i]);
                case "--seed" -> seed = Long.parseLong(args[++i]);
                case "--sweep" -> sweep = true;
                case "--no-verify" -> verify = false;
                case "--no-cross-check" -> cross = false;
                default -> throw new IllegalArgumentException("unknown option: " + args[i]);
            }
        }

        if (!CoolProp.isAvailable()) {
            System.err.println("error: CoolProp native library not loaded. Set COOLPROP_LIBRARY "
                    + "(run.sh does this for you).");
            System.exit(1);
        }
        String cpVersion = GenSupport.coolPropVersion();
        System.out.println("CoolProp " + cpVersion);

        Files.createDirectories(outDir);
        if (sweep) {
            sweep(outDir, fluids, cpVersion, samples, seed);
            return;
        }

        List<Map<String, Object>> manifest = new ArrayList<>();
        List<Map<String, Object>> reports = new ArrayList<>();
        for (String fluid : fluids) {
            System.out.println("== " + fluid);
            long t0 = System.nanoTime();
            Split split = new Split(fluid, nSat, nP, nDh, liquid, f32);
            System.out.printf(Locale.ROOT, "   built in %.1f s (%d CoolProp calls, %d nodes back-filled)%n",
                    (System.nanoTime() - t0) / 1e9, split.calls, split.backfilled);

            byte[] blob = serialize(split, cpVersion, f32);
            Path file = outDir.resolve(fluid.toLowerCase(Locale.ROOT) + ".phtab");
            Files.write(file, blob);
            System.out.printf(Locale.ROOT, "   wrote %s (%,d bytes)%n", file.getFileName(), blob.length);

            Map<String, Object> entry = manifestEntry(split, cpVersion, f32, file.getFileName().toString(), blob);
            if (cross) {
                entry.put("cross_check_vs_reference_max_rel", crossCheck(fluid, 400, seed));
            }
            manifest.add(entry);
            if (verify) {
                Report report = measure(split, samples, seed);
                report.print();
                reports.add(report.toMap());
            }
        }

        Files.writeString(outDir.resolve("MANIFEST.json"),
                GenSupport.json(Map.of(
                        "format", "FRPHTAB1",
                        "format_version", FORMAT_VERSION,
                        "generated_by", "tools/table-gen",
                        "coolprop_version", cpVersion,
                        "element_type", f32 ? "f32" : "f64",
                        "liquid_coord", liquid.name().toLowerCase(Locale.ROOT),
                        "tables", manifest)),
                StandardCharsets.UTF_8);
        System.out.println("wrote " + outDir.resolve("MANIFEST.json"));

        if (verify) {
            Files.writeString(outDir.resolve("ERROR-REPORT.json"),
                    GenSupport.json(Map.of(
                            "coolprop_version", cpVersion,
                            "samples", samples,
                            "seed", seed,
                            "fluids", reports)),
                    StandardCharsets.UTF_8);
            System.out.println("wrote " + outDir.resolve("ERROR-REPORT.json"));
        }
    }

    /** Measures a resolution ladder so the grid choice can be justified by numbers, not taste. */
    private static void sweep(Path outDir, List<String> fluids, String cpVersion, int samples, long seed)
            throws IOException {
        int[][] ladder = {{48, 24}, {96, 48}, {144, 72}, {192, 96}};
        int[] satLadder = {64, 128, 256, 512};
        List<Map<String, Object>> rows = new ArrayList<>();

        for (String fluid : fluids) {
            for (LiquidCoord coord : LiquidCoord.values()) {
                for (int[] res : ladder) {
                    Split s = new Split(fluid, 256, res[0], res[1], coord);
                    Report r = measure(s, samples, seed);
                    int bytes64 = serialize(s, cpVersion, false).length;
                    int bytes32 = serialize(s, cpVersion, true).length;
                    Map<String, Object> row = new LinkedHashMap<>();
                    row.put("fluid", fluid);
                    row.put("liquid_coord", coord.name().toLowerCase(Locale.ROOT));
                    row.put("n_sat", 256);
                    row.put("n_p", res[0]);
                    row.put("n_dh", res[1]);
                    row.put("bytes_f64", bytes64);
                    row.put("bytes_f32", bytes32);
                    row.putAll(r.toMap());
                    rows.add(row);
                    System.out.printf(Locale.ROOT,
                            "%-7s %-10s np=%3d ndh=%3d  %,8d B  cover=%5.1f%%  maxRel T=%.2e D=%.2e S=%.2e"
                                    + "  inv(P,T)=%.2e/n=%d  inv(P,s)=%.2e/n=%d%n",
                            fluid, coord.name().toLowerCase(Locale.ROOT), res[0], res[1], bytes64,
                            100.0 * r.covered / Math.max(1, r.total),
                            r.max[0], r.max[1], r.max[2],
                            r.maxInvT, r.countInvT, r.maxInvS, r.countInvS);
                }
            }
            // Saturation-line resolution, at the fixed 2-D grid, measured on the sat-line queries only.
            for (int ns : satLadder) {
                Split s = new Split(fluid, ns, 96, 48, LiquidCoord.NORMALIZED);
                Report r = measure(s, samples, seed);
                Map<String, Object> row = new LinkedHashMap<>();
                row.put("fluid", fluid);
                row.put("liquid_coord", "normalized");
                row.put("n_sat", ns);
                row.put("n_p", 96);
                row.put("n_dh", 48);
                row.put("bytes_f64", serialize(s, cpVersion, false).length);
                row.put("bytes_f32", serialize(s, cpVersion, true).length);
                row.putAll(r.toMap());
                rows.add(row);
                System.out.printf(Locale.ROOT,
                        "%-7s nsat=%3d              maxRel sat(P,x)=%.2e  sat(T,x)=%.2e  twophase T=%.2e%n",
                        fluid, ns, r.maxSatP, r.maxSatT, r.maxTwoPhaseT);
            }
        }
        Files.writeString(outDir.resolve("SWEEP.json"),
                GenSupport.json(Map.of("coolprop_version", cpVersion, "samples", samples, "seed", seed, "rows", rows)),
                StandardCharsets.UTF_8);
        System.out.println("wrote " + outDir.resolve("SWEEP.json"));
    }

    // ------------------------------------------------------- CoolProp version

    // ----------------------------------------------------------- split table

    /**
     * Resolution-parametric restatement of {@link SaturationSplitTable}, which
     * is package-private and hard-codes 256/96/48. The geometry is identical by
     * construction and {@link #crossCheck} proves it at that resolution; the 2-D
     * pieces are built by the real {@link PhPropertyTable}, so what is measured
     * here is the reference interpolant, not a stand-in.
     */
    static final class Split {

        final String fluid;
        final int nSat;
        final int nP;
        final int nDh;
        final LiquidCoord liquidCoord;
        /**
         * When set, every value that will be serialized is rounded to `f32`
         * <em>before</em> the interpolant is built, so the measured error
         * includes the storage quantization instead of pretending it away.
         */
        final boolean quantize;

        final double[] logP;
        final double[] tsat;
        final double[] hf;
        final double[] hg;
        final double[] vf;
        final double[] vfg;
        final double[] sf;
        final double[] sfg;
        /** {@code h(P, tLow)} — the cold end of the liquid sliver at each saturation pressure. */
        final double[] hcold;

        final double pMin;
        final double pMax;
        final double pServeMax;
        final double pLiquidMin;
        final double dhVaporMax;
        final double dhLiquidMax;
        final double hTop;
        final double tLow;
        final double pCrit;
        final double tCrit;
        final double pTriple;
        final double tTriple;

        final double[] vaporPGrid;
        final double[] vaporYGrid;
        final double[][][] vaporNodes;      // [output][iP][iY]
        final PhPropertyTable[] vapor;

        final double[] liquidPGrid;
        final double[] liquidYGrid;
        final double[][][] liquidNodes;
        final PhPropertyTable[] liquid;

        int backfilled;
        int calls;

        Split(String fluid, int nSat, int nP, int nDh, LiquidCoord liquidCoord) {
            this(fluid, nSat, nP, nDh, liquidCoord, false);
        }

        Split(String fluid, int nSat, int nP, int nDh, LiquidCoord liquidCoord, boolean quantize) {
            this.fluid = fluid;
            this.nSat = nSat;
            this.nP = nP;
            this.nDh = nDh;
            this.liquidCoord = liquidCoord;
            this.quantize = quantize;

            this.tCrit = CoolProp.props1SI(fluid, "Tcrit");
            this.pCrit = CoolProp.props1SI(fluid, "pcrit");
            this.pTriple = CoolProp.props1SI(fluid, "ptriple");
            this.tTriple = CoolProp.props1SI(fluid, "Ttriple");
            double tMin = CoolProp.props1SI(fluid, "Tmin");
            double tMax = CoolProp.props1SI(fluid, "Tmax");
            this.tLow = tMin + 1.0;
            double tHigh = Math.min(tMax, tCrit * 1.3);
            this.hTop = call("Hmass", "P", pCrit * 0.05, "T", tHigh);
            if (!Double.isFinite(hTop)) {
                throw new IllegalStateException(fluid + ": cannot establish hTop");
            }

            this.pMin = Math.max(pTriple * 1.2, pCrit * 1e-4);
            this.pMax = pCrit * 0.75;
            if (!(pMin > 0) || !(pMax > pMin)) {
                throw new IllegalStateException(fluid + ": no subcritical band");
            }
            this.pServeMax = pMax * 0.95;

            this.logP = new double[nSat];
            this.tsat = new double[nSat];
            this.hf = new double[nSat];
            this.hg = new double[nSat];
            this.vf = new double[nSat];
            this.vfg = new double[nSat];
            this.sf = new double[nSat];
            this.sfg = new double[nSat];
            this.hcold = new double[nSat];
            double logMin = Math.log(pMin);
            double logMax = Math.log(pMax);
            for (int i = 0; i < nSat; i++) {
                double p = Math.exp(logMin + (logMax - logMin) * i / (nSat - 1.0));
                logP[i] = Math.log(p);
                tsat[i] = call("T", "P", p, "Q", 0.0);
                hf[i] = call("Hmass", "P", p, "Q", 0.0);
                hg[i] = call("Hmass", "P", p, "Q", 1.0);
                double df = call("Dmass", "P", p, "Q", 0.0);
                double dg = call("Dmass", "P", p, "Q", 1.0);
                vf[i] = 1.0 / df;
                vfg[i] = 1.0 / dg - 1.0 / df;
                sf[i] = call("Smass", "P", p, "Q", 0.0);
                sfg[i] = call("Smass", "P", p, "Q", 1.0) - sf[i];
                hcold[i] = call("Hmass", "P", p, "T", tLow);
            }
            if (quantize) {
                for (double[] line : new double[][] {logP, tsat, hf, hg, vf, vfg, sf, sfg, hcold}) {
                    toF32(line);
                }
            }

            double hgMax = 0.0;
            for (double v : hg) {
                hgMax = Math.max(hgMax, v);
            }
            this.dhVaporMax = 0.9 * (hTop - hgMax);
            if (!(dhVaporMax > 0)) {
                throw new IllegalStateException(fluid + ": no superheat band under hTop");
            }

            // Liquid rectangle. ABSOLUTE reproduces the reference: one depth
            // that is valid at every served pressure, so it is bounded by the
            // *thinnest* sliver (low P) and leaves cold high-pressure liquid
            // uncovered. NORMALIZED rescales the depth per pressure instead.
            boolean normalized = liquidCoord == LiquidCoord.NORMALIZED;
            int liquidStart = -1;
            double depth = Double.POSITIVE_INFINITY;
            for (int i = 0; i < nSat; i++) {
                if (tsat[i] < tLow + 5.0) {
                    continue;
                }
                if (!Double.isFinite(hcold[i])) {
                    continue;
                }
                double d = hf[i] - hcold[i];
                // ABSOLUTE needs one depth valid everywhere, so it must wait for
                // the sliver to be worth resolving. NORMALIZED rescales per
                // pressure, so any non-degenerate sliver is enough.
                if (liquidStart < 0 && (normalized ? d > 0 : d >= MIN_LIQUID_DEPTH)) {
                    liquidStart = i;
                }
                if (liquidStart >= 0) {
                    depth = Math.min(depth, d);
                }
            }
            if (liquidStart >= 0 && Double.isFinite(depth)) {
                this.pLiquidMin = Math.exp(logP[liquidStart]);
                this.dhLiquidMax = normalized ? 0.9 : 0.9 * depth;
            } else {
                this.pLiquidMin = Double.POSITIVE_INFINITY;
                this.dhLiquidMax = 0.0;
            }

            this.vaporPGrid = maybeF32(logspace(pMin, pMax, nP));
            this.vaporYGrid = maybeF32(squarespace(dhVaporMax / 0.9, nDh));
            this.vaporNodes = new double[OUTPUTS.length][][];
            this.vapor = new PhPropertyTable[OUTPUTS.length];
            for (int k = 0; k < OUTPUTS.length; k++) {
                vaporNodes[k] = sample(OUTPUTS[k], vaporPGrid, vaporYGrid, true);
                vapor[k] = table(vaporPGrid, vaporYGrid, vaporNodes[k]);
            }

            if (dhLiquidMax > 0) {
                this.liquidPGrid = maybeF32(logspace(pLiquidMin, pMax, nP));
                this.liquidYGrid = maybeF32(squarespace(dhLiquidMax / 0.9, nDh));
                this.liquidNodes = new double[OUTPUTS.length][][];
                this.liquid = new PhPropertyTable[OUTPUTS.length];
                for (int k = 0; k < OUTPUTS.length; k++) {
                    liquidNodes[k] = sample(OUTPUTS[k], liquidPGrid, liquidYGrid, false);
                    liquid[k] = table(liquidPGrid, liquidYGrid, liquidNodes[k]);
                }
            } else {
                this.liquidPGrid = null;
                this.liquidYGrid = null;
                this.liquidNodes = null;
                this.liquid = null;
            }
        }

        boolean hasLiquid() {
            return liquid != null;
        }

        /** Enthalpy of the node at depth {@code y} on the given side of the dome. */
        double hAt(double p, double y, boolean vapor) {
            if (vapor) {
                return hgAt(p) + y;
            }
            double hfv = hfAt(p);
            return liquidCoord == LiquidCoord.NORMALIZED
                    ? hfv - y * (hfv - hcoldAt(p))
                    : hfv - y;
        }

        private double[][] sample(String output, double[] pGrid, double[] yGrid, boolean vapor) {
            double[][] raw = new double[pGrid.length][yGrid.length];
            for (int i = 0; i < pGrid.length; i++) {
                for (int j = 0; j < yGrid.length; j++) {
                    raw[i][j] = call(output, "P", pGrid[i], "Hmass", hAt(pGrid[i], yGrid[j], vapor));
                }
            }
            backfilled += fillNonFinite(raw);
            if (quantize) {
                for (double[] row : raw) {
                    toF32(row);
                }
            }
            return raw;
        }

        private double[] maybeF32(double[] a) {
            if (quantize) {
                toF32(a);
            }
            return a;
        }

        private static void toF32(double[] a) {
            for (int i = 0; i < a.length; i++) {
                a[i] = (float) a[i];
            }
        }

        /**
         * Wraps already-sampled nodes in the reference {@link PhPropertyTable}.
         * {@code build} evaluates the sampler at exactly the grid values it was
         * handed, so an exact-match index lookup is well defined — and the
         * serialized nodes are then bit-identical to the ones the interpolant
         * holds.
         */
        private static PhPropertyTable table(double[] pGrid, double[] yGrid, double[][] nodes) {
            DoubleBinaryOperator sampler = (p, y) -> nodes[exactIndex(pGrid, p)][exactIndex(yGrid, y)];
            return PhPropertyTable.build(pGrid, yGrid, sampler);
        }

        private double call(String output, String k1, double v1, String k2, double v2) {
            calls++;
            return CoolProp.propsSIOrNaN(output, k1, v1, k2, v2, fluid);
        }

        // --- lookup, transcribed from SaturationSplitTable.value ------------

        /** {@code OUTPUTS[out](P,h)}, or {@code null} when the point is not covered. */
        Double value(int out, double p, double h) {
            if (p < pMin || p > pServeMax) {
                return null;
            }
            double hfv = hfAt(p);
            double hgv = hgAt(p);
            if (h >= hfv && h <= hgv) {
                double x = (h - hfv) / (hgv - hfv);
                return switch (out) {
                    case 0 -> tsatAt(p);
                    case 1 -> 1.0 / (interp(vf, p) + x * interp(vfg, p));
                    case 2 -> interp(sf, p) + x * interp(sfg, p);
                    default -> null;
                };
            }
            if (h > hgv) {
                double y = h - hgv;
                if (y > dhVaporMax) {
                    return null;
                }
                return vapor[out].value(p, y);
            }
            if (liquid == null || p < pLiquidMin) {
                return null;
            }
            double y = liquidCoord == LiquidCoord.NORMALIZED
                    ? (hfv - h) / (hfv - hcoldAt(p))
                    : hfv - h;
            if (y > dhLiquidMax || !Double.isFinite(y)) {
                return null;
            }
            return liquid[out].value(p, y);
        }

        double tsatAt(double p) {
            return interp(tsat, p);
        }

        double hfAt(double p) {
            return interp(hf, p);
        }

        double hgAt(double p) {
            return interp(hg, p);
        }

        double hcoldAt(double p) {
            return interp(hcold, p);
        }

        /** Saturated-liquid specific volume on the line. */
        double vfAt(double p) {
            return interp(vf, p);
        }

        /** Saturated-liquid entropy on the line. */
        double sfAt(double p) {
            return interp(sf, p);
        }

        /** Saturation pressure at {@code T}, by monotone inversion of {@code Tsat(logP)}. */
        Double pSat(double t) {
            if (t < tsat[0] || t > tsat[nSat - 1]) {
                return null;
            }
            double lo = logP[0];
            double hi = logP[nSat - 1];
            for (int k = 0; k < 80; k++) {
                double mid = 0.5 * (lo + hi);
                if (interp(tsat, Math.exp(mid)) < t) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return Math.exp(0.5 * (lo + hi));
        }

        /**
         * {@code h} such that {@code OUTPUTS[out](P,h) = target}, by bisection on
         * the tabulated surface. This is the query form the pending Rankine and
         * refrigeration documents actually use — {@code Enthalpy(Water, P=…, T=…)}
         * and {@code Enthalpy(Water, P=…, s=…)} — and the reference's own table
         * path never exercises it, because {@code PhTableRegistry} only intercepts
         * {@code (P, Hmass)} inputs and lets everything else reach CoolProp.
         */
        Double invert(int out, double p, double target, boolean vaporSide) {
            if (p < pMin || p > pServeMax) {
                return null;
            }
            // The bracket ends sit exactly on the serve limit, where `y` can round
            // a single ulp past `dh*Max` and `value` then answers "uncovered" —
            // which silently turned every water inversion into a no-op. Pull the
            // far end inside by a relative hair.
            double inside = 1.0 - 1e-9;
            double lo;
            double hi;
            if (vaporSide) {
                lo = hgAt(p);
                hi = lo + dhVaporMax * inside;
            } else {
                if (liquid == null || p < pLiquidMin) {
                    return null;
                }
                hi = hfAt(p);
                lo = liquidCoord == LiquidCoord.NORMALIZED
                        ? hi - dhLiquidMax * inside * (hi - hcoldAt(p))
                        : hi - dhLiquidMax * inside;
            }
            Double flo = value(out, p, lo);
            Double fhi = value(out, p, hi);
            if (flo == null || fhi == null) {
                return null;
            }
            boolean rising = fhi > flo;
            if (target < Math.min(flo, fhi) || target > Math.max(flo, fhi)) {
                return null;
            }
            for (int k = 0; k < 100; k++) {
                double mid = 0.5 * (lo + hi);
                Double f = value(out, p, mid);
                if (f == null) {
                    return null;
                }
                if ((f < target) == rising) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return 0.5 * (lo + hi);
        }

        /** Cubic-Hermite on the log-P saturation grid, transcribed from the reference. */
        double interp(double[] f, double p) {
            double x = Math.log(p);
            int lo = 0;
            int hi = nSat - 1;
            while (hi - lo > 1) {
                int mid = (lo + hi) >>> 1;
                if (logP[mid] <= x) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            double x0 = logP[lo];
            double x1 = logP[hi];
            double t = Math.clamp((x - x0) / (x1 - x0), 0.0, 1.0);
            double m0 = slope(f, lo);
            double m1 = slope(f, hi);
            double dx = x1 - x0;
            double t2 = t * t;
            double t3 = t2 * t;
            return (2 * t3 - 3 * t2 + 1) * f[lo] + (t3 - 2 * t2 + t) * dx * m0
                    + (-2 * t3 + 3 * t2) * f[hi] + (t3 - t2) * dx * m1;
        }

        private double slope(double[] f, int i) {
            if (i == 0) {
                return (f[1] - f[0]) / (logP[1] - logP[0]);
            }
            if (i == nSat - 1) {
                return (f[nSat - 1] - f[nSat - 2]) / (logP[nSat - 1] - logP[nSat - 2]);
            }
            return (f[i + 1] - f[i - 1]) / (logP[i + 1] - logP[i - 1]);
        }
    }

    /**
     * Proves the restated geometry against the real {@link SaturationSplitTable}
     * at the reference resolution. Any disagreement means the restatement drifted
     * and every number downstream is suspect.
     */
    static double crossCheck(String fluid, int samples, long seed) {
        Split mine = new Split(fluid, 256, 96, 48, LiquidCoord.ABSOLUTE);
        SaturationSplitTable ref = new SaturationSplitTable(fluid, mine.hTop, mine.tLow);
        Random random = new Random(seed);
        double worst = 0.0;
        int compared = 0;
        for (int k = 0; k < samples; k++) {
            double p = Math.exp(Math.log(mine.pMin)
                    + random.nextDouble() * (Math.log(mine.pServeMax) - Math.log(mine.pMin)));
            double h = mine.hfAt(p) - 1.5e5 + random.nextDouble() * (mine.hgAt(p) + 5.0e5 - mine.hfAt(p) + 1.5e5);
            for (int out = 0; out < OUTPUTS.length; out++) {
                Double a = mine.value(out, p, h);
                Double b = ref.value(OUTPUTS[out], p, h);
                if (a == null || b == null) {
                    continue;
                }
                compared++;
                worst = Math.max(worst, Math.abs(a - b) / Math.max(Math.abs(b), 1e-30));
            }
        }
        System.out.printf(Locale.ROOT,
                "   cross-check vs reference SaturationSplitTable: %d points, worst rel diff %.3e%n",
                compared, worst);
        return worst;
    }

    // ------------------------------------------------------------ validation

    /** Measured tabulation error for one fluid. */
    static final class Report {
        String fluid;
        int total;
        int covered;
        /**
         * Samples the table declines <em>that CoolProp can actually evaluate</em>.
         * Plain "uncovered" over-counts badly: the sampling band runs to
         * {@code h_g + 500 kJ/kg}, which for a fluid with a low {@code Tmax}
         * (R134a: 455 K) is mostly states that do not exist. This is the number
         * that means "the browser build would have no answer here".
         */
        int trueMisses;
        final double[] max = new double[OUTPUTS.length];
        final double[] rms = new double[OUTPUTS.length];
        final int[] count = new int[OUTPUTS.length];
        final double[] maxByRegion = new double[3];   // liquid, two-phase, vapor
        final int[] countByRegion = new int[3];
        double maxInvT;
        double rmsInvT;
        int countInvT;
        double maxInvS;
        double rmsInvS;
        int countInvS;
        double maxSatP;
        double maxSatT;
        double maxTwoPhaseT;
        final List<Map<String, Object>> docStates = new ArrayList<>();

        void print() {
            System.out.printf(Locale.ROOT,
                    "   coverage %d/%d = %.1f%% of the sampling band; %d true misses "
                            + "(declined but CoolProp-evaluable) = %.1f%%%n",
                    covered, total, 100.0 * covered / Math.max(1, total),
                    trueMisses, 100.0 * trueMisses / Math.max(1, total));
            for (int k = 0; k < OUTPUTS.length; k++) {
                System.out.printf(Locale.ROOT, "   (P,h) -> %-6s  n=%5d  max %.3e  rms %.3e%n",
                        OUTPUTS[k], count[k], max[k], rms[k]);
            }
            System.out.printf(Locale.ROOT, "     by region: liquid %.3e (n=%d)  two-phase %.3e (n=%d)  vapor %.3e (n=%d)%n",
                    maxByRegion[0], countByRegion[0], maxByRegion[1], countByRegion[1],
                    maxByRegion[2], countByRegion[2]);
            System.out.printf(Locale.ROOT, "   (P,T) -> h     n=%5d  max %.3e  rms %.3e%n", countInvT, maxInvT, rmsInvT);
            System.out.printf(Locale.ROOT, "   (P,s) -> h     n=%5d  max %.3e  rms %.3e%n", countInvS, maxInvS, rmsInvS);
            System.out.printf(Locale.ROOT, "   (P,x) sat line max %.3e   (T,x) sat line max %.3e%n", maxSatP, maxSatT);
            for (Map<String, Object> d : docStates) {
                System.out.printf(Locale.ROOT, "   doc %-42s %s%n", d.get("label"), d.get("errors"));
            }
        }

        Map<String, Object> toMap() {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("fluid", fluid);
            m.put("samples", total);
            m.put("covered", covered);
            m.put("true_misses", trueMisses);
            Map<String, Object> ph = new LinkedHashMap<>();
            for (int k = 0; k < OUTPUTS.length; k++) {
                ph.put(OUTPUTS[k], Map.of("n", count[k], "max_rel", max[k], "rms_rel", rms[k]));
            }
            m.put("forward_ph", ph);
            m.put("by_region", Map.of(
                    "liquid", Map.of("n", countByRegion[0], "max_rel", maxByRegion[0]),
                    "two_phase", Map.of("n", countByRegion[1], "max_rel", maxByRegion[1]),
                    "vapor", Map.of("n", countByRegion[2], "max_rel", maxByRegion[2])));
            m.put("inverse_pt_to_h", Map.of("n", countInvT, "max_rel", maxInvT, "rms_rel", rmsInvT));
            m.put("inverse_ps_to_h", Map.of("n", countInvS, "max_rel", maxInvS, "rms_rel", rmsInvS));
            m.put("saturation_px", maxSatP);
            m.put("saturation_tx", maxSatT);
            m.put("two_phase_T", maxTwoPhaseT);
            m.put("document_states", docStates);
            return m;
        }
    }

    /**
     * Samples the covered band at fixed seed and compares the table against
     * direct CoolProp — forward {@code (P,h)}, both inversions, and both
     * saturation-line query forms — then replays the concrete states the pending
     * fluid fixtures land on.
     */
    static Report measure(Split s, int samples, long seed) {
        Report r = new Report();
        r.fluid = s.fluid;
        Random random = new Random(seed);
        double logMin = Math.log(s.pMin);
        double logMax = Math.log(s.pServeMax);
        double[] sum = new double[OUTPUTS.length];

        for (int k = 0; k < samples; k++) {
            double p = Math.exp(logMin + random.nextDouble() * (logMax - logMin));
            double hfv = CoolProp.propsSIOrNaN("Hmass", "P", p, "Q", 0.0, s.fluid);
            double hgv = CoolProp.propsSIOrNaN("Hmass", "P", p, "Q", 1.0, s.fluid);
            if (!Double.isFinite(hfv) || !Double.isFinite(hgv)) {
                continue;
            }
            double hLo = hfv - 1.5e5;
            double hHi = hgv + 5.0e5;
            double h = hLo + random.nextDouble() * (hHi - hLo);
            r.total++;
            int region = h < hfv ? 0 : (h <= hgv ? 1 : 2);

            boolean anyCovered = false;
            for (int out = 0; out < OUTPUTS.length; out++) {
                Double tab = s.value(out, p, h);
                if (tab == null) {
                    continue;
                }
                double direct = CoolProp.propsSIOrNaN(OUTPUTS[out], "P", p, "Hmass", h, s.fluid);
                if (!Double.isFinite(direct)) {
                    continue;
                }
                anyCovered = true;
                double err = Math.abs(tab - direct) / Math.max(Math.abs(direct), 1e-12);
                r.count[out]++;
                sum[out] += err * err;
                r.max[out] = Math.max(r.max[out], err);
                r.maxByRegion[region] = Math.max(r.maxByRegion[region], err);
                if (region == 1 && out == 0) {
                    r.maxTwoPhaseT = Math.max(r.maxTwoPhaseT, err);
                }
            }
            if (anyCovered) {
                r.covered++;
                r.countByRegion[region]++;
            } else if (Double.isFinite(CoolProp.propsSIOrNaN("T", "P", p, "Hmass", h, s.fluid))) {
                r.trueMisses++;
            }

            // Inverse forms, on whichever single-phase side the sample sits.
            // Both sides are load-bearing: rankine-cycle's h3 is a (P,T) look-up
            // in superheated steam, state-tables' hw_1 is a (P,T) look-up in
            // subcooled liquid. Inside the dome (P,T) does not determine h at
            // all, so region 1 is skipped.
            if (region != 1) {
                boolean vaporSide = region == 2;
                double tDirect = CoolProp.propsSIOrNaN("T", "P", p, "Hmass", h, s.fluid);
                if (Double.isFinite(tDirect)) {
                    Double hBack = s.invert(0, p, tDirect, vaporSide);
                    if (hBack != null) {
                        double err = Math.abs(hBack - h) / Math.max(Math.abs(h), 1e-12);
                        r.countInvT++;
                        r.rmsInvT += err * err;
                        r.maxInvT = Math.max(r.maxInvT, err);
                    }
                }
                double sDirect = CoolProp.propsSIOrNaN("Smass", "P", p, "Hmass", h, s.fluid);
                if (Double.isFinite(sDirect)) {
                    Double hBack = s.invert(2, p, sDirect, vaporSide);
                    if (hBack != null) {
                        double err = Math.abs(hBack - h) / Math.max(Math.abs(h), 1e-12);
                        r.countInvS++;
                        r.rmsInvS += err * err;
                        r.maxInvS = Math.max(r.maxInvS, err);
                    }
                }
            }

            // Saturation-line forms: (P,x) straight off the lines, (T,x) after
            // inverting Tsat.
            double x = random.nextDouble();
            double hSatDirect = CoolProp.propsSIOrNaN("Hmass", "P", p, "Q", x, s.fluid);
            if (Double.isFinite(hSatDirect)) {
                double hSatTab = s.hfAt(p) + x * (s.hgAt(p) - s.hfAt(p));
                r.maxSatP = Math.max(r.maxSatP, Math.abs(hSatTab - hSatDirect) / Math.max(Math.abs(hSatDirect), 1e-12));
            }
            double tq = s.tsatAt(p);
            Double pBack = s.pSat(tq);
            double pDirect = CoolProp.propsSIOrNaN("P", "T", tq, "Q", 0.0, s.fluid);
            if (pBack != null && Double.isFinite(pDirect)) {
                r.maxSatT = Math.max(r.maxSatT, Math.abs(pBack - pDirect) / Math.max(Math.abs(pDirect), 1e-12));
            }
        }

        // A statistic over zero samples is NaN, never 0.0 — an unmeasured query
        // form reading as "exact" is how a broken measurement gets published.
        for (int k = 0; k < OUTPUTS.length; k++) {
            r.rms[k] = r.count[k] == 0 ? Double.NaN : Math.sqrt(sum[k] / r.count[k]);
            if (r.count[k] == 0) {
                r.max[k] = Double.NaN;
            }
        }
        r.rmsInvT = r.countInvT == 0 ? Double.NaN : Math.sqrt(r.rmsInvT / r.countInvT);
        r.rmsInvS = r.countInvS == 0 ? Double.NaN : Math.sqrt(r.rmsInvS / r.countInvS);
        if (r.countInvT == 0) {
            r.maxInvT = Double.NaN;
        }
        if (r.countInvS == 0) {
            r.maxInvS = Double.NaN;
        }
        for (int i = 0; i < r.countByRegion.length; i++) {
            if (r.countByRegion[i] == 0) {
                r.maxByRegion[i] = Double.NaN;
            }
        }

        for (DocState d : DOC_STATES) {
            if (!d.fluid.equals(s.fluid)) {
                continue;
            }
            Map<String, Object> entry = new LinkedHashMap<>();
            entry.put("label", d.label);
            entry.put("P", d.p);
            entry.put("h", d.h);
            Map<String, Object> errors = new LinkedHashMap<>();
            for (int out = 0; out < OUTPUTS.length; out++) {
                Double tab = s.value(out, d.p, d.h);
                double direct = CoolProp.propsSIOrNaN(OUTPUTS[out], "P", d.p, "Hmass", d.h, s.fluid);
                if (tab == null) {
                    errors.put(OUTPUTS[out], "uncovered");
                } else if (!Double.isFinite(direct)) {
                    errors.put(OUTPUTS[out], "no-direct-value");
                } else {
                    errors.put(OUTPUTS[out], Math.abs(tab - direct) / Math.max(Math.abs(direct), 1e-12));
                }
            }
            entry.put("errors", errors);
            r.docStates.add(entry);
        }
        return r;
    }

    /** A concrete state one of the pending fluid fixtures lands on. */
    record DocState(String fluid, double p, double h, String label) {
    }

    /**
     * States taken from the regenerated goldens in
     * {@code fixtures/corpus-pending/golden/} — the numbers the Rust engine has
     * to reproduce, not synthetic points.
     */
    static final DocState[] DOC_STATES = {
            new DocState("Water", 10000.0, 191805.94455889906, "rankine-cycle h1: sat liquid @10 kPa"),
            new DocState("Water", 8000000.0, 199878.01103600737, "rankine-cycle h2: pump exit @8 MPa"),
            new DocState("Water", 8000000.0, 3349645.1659218343, "rankine-cycle h3: 480 C steam @8 MPa"),
            new DocState("Water", 10000.0, 2109393.1272094045, "rankine-cycle h4: wet steam @10 kPa"),
            new DocState("Water", 10000.0, 188435.13550803528, "state-tables hw_1: 45 C liquid @10 kPa"),
            new DocState("R134a", 200603.30747267744, 392664.91356786405, "vcr h1: sat vapor @-10 C"),
            new DocState("R134a", 1016593.02212064, 426479.692738112, "vcr h2s: isentropic comp. exit"),
            new DocState("R134a", 1016593.02212064, 434933.387530674, "vcr h2: real comp. exit"),
            new DocState("R134a", 1016593.02212064, 256409.2445573684, "vcr h3: sat liquid @40 C"),
            new DocState("R134a", 200603.30747267744, 256409.2445573684, "vcr h4: post-throttle two-phase"),
            new DocState("R134a", 200000.0, 392618.89553575753, "state-tables href_1: sat vapor @200 kPa"),
            new DocState("R134a", 1200000.0, 437787.9473383412, "state-tables href_2: 60 C vapor @1.2 MPa"),
            new DocState("R134a", 1200000.0, 265947.2005481485, "ev-battery hliq: sat liquid @1.2 MPa"),
    };

    // --------------------------------------------------------- serialization

    /** Writes one fluid's split table in the {@code FRPHTAB1} format (see README.md). */
    static byte[] serialize(Split s, String cpVersion, boolean f32) {
        byte[] fluidBytes = s.fluid.getBytes(StandardCharsets.UTF_8);
        byte[] versionBytes = cpVersion.getBytes(StandardCharsets.UTF_8);
        int headerBytes = 144 + fluidBytes.length + versionBytes.length;
        headerBytes = (headerBytes + 7) & ~7;

        GenSupport.Buf b = new GenSupport.Buf();
        b.bytes(MAGIC);
        b.u16(FORMAT_VERSION);
        b.u8(f32 ? 1 : 0);
        int flags = (s.hasLiquid() ? 1 : 0) | (s.liquidCoord == LiquidCoord.NORMALIZED ? 2 : 0);
        b.u8(flags);
        b.u32(s.nSat);
        b.u32(s.nP);
        b.u32(s.nDh);
        b.u32(OUTPUTS.length);
        b.u32(headerBytes);
        b.f64(s.pMin);
        b.f64(s.pMax);
        b.f64(s.pServeMax);
        b.f64(s.pLiquidMin);
        b.f64(s.dhVaporMax);
        b.f64(s.dhLiquidMax);
        b.f64(s.hTop);
        b.f64(s.tLow);
        b.f64(s.pCrit);
        b.f64(s.tCrit);
        b.f64(s.pTriple);
        b.f64(s.tTriple);
        b.u16(fluidBytes.length);
        b.u16(versionBytes.length);
        b.u32(s.backfilled);
        b.bytes(fluidBytes);
        b.bytes(versionBytes);
        while (b.size() < headerBytes) {
            b.u8(0);
        }

        for (double[] line : new double[][] {s.logP, s.tsat, s.hf, s.hg, s.vf, s.vfg, s.sf, s.sfg, s.hcold}) {
            b.array(line, f32);
        }
        b.array(s.vaporPGrid, f32);
        b.array(s.vaporYGrid, f32);
        for (double[][] plane : s.vaporNodes) {
            for (double[] row : plane) {
                b.array(row, f32);
            }
        }
        if (s.hasLiquid()) {
            b.array(s.liquidPGrid, f32);
            b.array(s.liquidYGrid, f32);
            for (double[][] plane : s.liquidNodes) {
                for (double[] row : plane) {
                    b.array(row, f32);
                }
            }
        }
        return b.toByteArray();
    }

    private static Map<String, Object> manifestEntry(Split s, String cpVersion, boolean f32,
                                                     String fileName, byte[] blob) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("fluid", s.fluid);
        m.put("file", fileName);
        m.put("bytes", blob.length);
        m.put("sha256", GenSupport.sha256(blob));
        m.put("element_type", f32 ? "f32" : "f64");
        m.put("coolprop_version", cpVersion);
        m.put("n_sat", s.nSat);
        m.put("n_p", s.nP);
        m.put("n_dh", s.nDh);
        m.put("outputs", List.of(OUTPUTS));
        m.put("liquid_coord", s.liquidCoord.name().toLowerCase(Locale.ROOT));
        m.put("liquid_piece", s.hasLiquid());
        m.put("backfilled_nodes", s.backfilled);
        m.put("coolprop_calls", s.calls);
        Map<String, Object> bounds = new LinkedHashMap<>();
        bounds.put("p_min", s.pMin);
        bounds.put("p_max", s.pMax);
        bounds.put("p_serve_max", s.pServeMax);
        bounds.put("p_liquid_min", s.pLiquidMin);
        bounds.put("dh_vapor_max", s.dhVaporMax);
        bounds.put("dh_liquid_max", s.dhLiquidMax);
        bounds.put("h_top", s.hTop);
        bounds.put("t_low", s.tLow);
        bounds.put("t_sat_min", s.tsat[0]);
        bounds.put("t_sat_max", s.tsat[s.nSat - 1]);
        bounds.put("p_crit", s.pCrit);
        bounds.put("t_crit", s.tCrit);
        bounds.put("p_triple", s.pTriple);
        bounds.put("t_triple", s.tTriple);
        m.put("bounds", bounds);
        return m;
    }

    /** Little-endian byte sink. */
    // ------------------------------------------------------------- utilities

    /** Transcribed from {@code PhPropertyTable.fillNonFinite}; returns how many nodes were repaired. */
    private static int fillNonFinite(double[][] f) {
        int np = f.length;
        int nh = f[0].length;
        int repaired = 0;
        for (int i = 0; i < np; i++) {
            for (int j = 0; j < nh; j++) {
                if (!Double.isFinite(f[i][j])) {
                    f[i][j] = nearestFinite(f, i, j, np, nh);
                    repaired++;
                }
            }
        }
        return repaired;
    }

    private static double nearestFinite(double[][] f, int i, int j, int np, int nh) {
        for (int r = 1; r < Math.max(np, nh); r++) {
            for (int di = -r; di <= r; di++) {
                for (int dj = -r; dj <= r; dj++) {
                    int ni = i + di;
                    int nj = j + dj;
                    if (ni >= 0 && ni < np && nj >= 0 && nj < nh && Double.isFinite(f[ni][nj])) {
                        return f[ni][nj];
                    }
                }
            }
        }
        return 0.0;
    }

    private static int exactIndex(double[] grid, double v) {
        int i = Arrays.binarySearch(grid, v);
        if (i < 0) {
            throw new IllegalStateException("grid value not found: " + v);
        }
        return i;
    }

    private static double[] logspace(double lo, double hi, int n) {
        double[] out = new double[n];
        double a = Math.log(lo);
        double b = Math.log(hi);
        for (int i = 0; i < n; i++) {
            out[i] = Math.exp(a + (b - a) * i / (n - 1.0));
        }
        return out;
    }

    private static double[] squarespace(double max, int n) {
        double[] out = new double[n];
        for (int i = 0; i < n; i++) {
            double t = i / (n - 1.0);
            out[i] = max * t * t;
        }
        return out;
    }

}
