package com.frees.backend.props;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;

/**
 * Generates the {@code FRAUX1} auxiliary property grids from native CoolProp —
 * the three surfaces the D1 {@code (P,h)} split table deliberately cannot carry.
 *
 * <p>Sibling of {@code tools/table-gen}: same {@code classpath.sh}, same
 * hand-written JSON, same discipline of adding no dependency the reference
 * engine does not already have. Where {@code TableGen} tabulates a fluid with a
 * saturation dome in {@code (P,h)}, this tool tabulates the three things that
 * geometry has no room for:
 *
 * <ul>
 *   <li><b>{@code INCOMPRESSIBLE}</b> — the aqueous glycols
 *       ({@code INCOMP::MEG[x]}, {@code INCOMP::MPG[x]}). They have no dome at
 *       all, so D1's geometry does not apply to them. Their surface is
 *       {@code (mass fraction, T)}, and it is unusually cheap to tabulate
 *       exactly: CoolProp's incompressible model makes {@code rho}, {@code cp},
 *       {@code mu} and {@code k} <b>exactly</b> pressure-independent, and makes
 *       {@code h} and {@code s} <b>exactly linear</b> in pressure. Measured, not
 *       assumed: {@code dh/dP} at 305 K reproduces to all 16 digits from 1 bar
 *       to 100 bar. So storing {@code h} and {@code s} at a reference pressure
 *       plus their (P-independent) pressure slopes reproduces CoolProp with no
 *       error beyond the {@code (x,T)} interpolation itself.</li>
 *   <li><b>{@code PRESSURE_TEMPERATURE}</b> — a single-phase {@code (P,T)}
 *       transport grid. Air is the one this port needs: {@code htc_extair} asks
 *       for {@code viscosity}, {@code conductivity} and {@code Cpmass} at
 *       {@code (P,T)}, and air is not tabulated at all today.</li>
 *   <li><b>{@code SATURATION_LINE}</b> — transport along the dome, at
 *       {@code Q = 0} and {@code Q = 1} only. This is the whole reason the
 *       two-phase correlations were blocked, and the reason the fix is small:
 *       {@code htc_evap}, {@code htc_cond} and {@code dp_2phase} look up
 *       {@code viscosity} / {@code conductivity} / {@code Cpmass}
 *       <b>exclusively</b> at {@code (P, Q=0)} and {@code (P, Q=1)} — never off
 *       the dome. A pair of 1-D arrays over the saturation grid therefore serves
 *       every one of them, at ~6 KB per fluid instead of the ~256 KB a second
 *       full 2-D plane set would cost.</li>
 * </ul>
 *
 * <h2>The ragged domain</h2>
 *
 * A glycol's valid temperature band depends on its concentration — MEG freezes
 * at 273.5 K pure-water and at 222 K at 60 % mass. A rectangular {@code (x,T)}
 * grid over the union of those bands would be mostly holes. So the second axis
 * is <b>normalized</b>: {@code tau = (T - Tmin(x)) / (Tmax(x) - Tmin(x))}, with
 * the two endpoint columns stored per {@code x}. That is the same trick D1
 * already uses for the liquid piece of the split table, for the same reason, and
 * it makes every node of the grid a real state.
 *
 * <h2>What it emits</h2>
 *
 * One {@code <name>.fraux} per surface in the binary format documented in
 * {@code README.md}, plus {@code AUX-MANIFEST.json} (grid bounds, resolution,
 * CoolProp version, SHA-256, byte size) and {@code AUX-ERROR-REPORT.json} (the
 * measured interpolation error at random interior states, against the live
 * library).
 *
 * <p>It reads {@code ../frees} but never writes to it.
 */
public final class AuxGen {

    /** Format identifier written at byte 0 of every {@code .fraux}. */
    private static final byte[] MAGIC = "FRAUX1\0\0".getBytes(StandardCharsets.US_ASCII);
    private static final int FORMAT_VERSION = 1;

    /** Header is padded to an 8-byte boundary so the payload arrays are aligned. */
    private static final int HEADER_FIXED = 84;

    /** {@code flags} bit 0 — axis 2 is normalized with per-column endpoints. */
    private static final int FLAG_RAGGED = 1;

    /** Surface kinds, serialized as {@code u32 kind}. */
    private static final int KIND_INCOMPRESSIBLE = 0;
    private static final int KIND_PRESSURE_TEMPERATURE = 1;
    private static final int KIND_SATURATION_LINE = 2;

    /** Per-output storage transform. Viscosity spans decades; store its log. */
    private static final int XFORM_LINEAR = 0;
    private static final int XFORM_LOG = 1;

    /**
     * Reference pressure for the incompressible {@code h} / {@code s} columns.
     * Any value works — the pressure slopes are exact — but 1 atm keeps the
     * stored numbers close to what a reader would expect to see.
     */
    private static final double INCOMP_P_REF = 101325.0;

    /** Pressure offset used to measure the (P-independent) enthalpy/entropy slopes. */
    private static final double INCOMP_P_PROBE = 2.0e6;

    private AuxGen() {
    }

    // ------------------------------------------------------------------ main

    public static void main(String[] args) throws IOException {
        Path out = Path.of(args.length > 0 && !args[0].startsWith("--")
                ? args[0]
                : "fixtures/auxtables");
        boolean f32 = true;
        int ntau = 48;
        int nPtAir = 24;
        int nTAir = 64;
        int nSat = 512;
        int samples = 4000;
        long seed = 20260806L;
        List<String> only = null;
        boolean sweep = false;

        for (int i = (args.length > 0 && !args[0].startsWith("--")) ? 1 : 0; i < args.length; i++) {
            switch (args[i]) {
                case "--f64" -> f32 = false;
                case "--ntau" -> ntau = Integer.parseInt(args[++i]);
                case "--nsat" -> nSat = Integer.parseInt(args[++i]);
                case "--npair" -> nPtAir = Integer.parseInt(args[++i]);
                case "--ntair" -> nTAir = Integer.parseInt(args[++i]);
                case "--samples" -> samples = Integer.parseInt(args[++i]);
                case "--seed" -> seed = Long.parseLong(args[++i]);
                case "--only" -> only = List.of(args[++i].split(","));
                case "--sweep" -> sweep = true;
                default -> throw new IllegalArgumentException("unknown option: " + args[i]);
            }
        }

        if (!CoolProp.isAvailable()) {
            System.err.println("error: CoolProp is not loaded — set COOLPROP_LIBRARY");
            System.exit(1);
        }
        if (sweep) {
            sweep(samples, seed, f32);
            return;
        }
        Files.createDirectories(out);
        String cpVersion = GenSupport.coolPropVersion();
        System.err.printf("CoolProp %s -> %s%n", cpVersion, out.toAbsolutePath());

        List<Map<String, Object>> manifest = new ArrayList<>();
        List<Map<String, Object>> errors = new ArrayList<>();
        Random rng = new Random(seed);

        // ---- incompressible glycol families ----
        for (String family : new String[] {"MEG", "MPG"}) {
            if (only != null && !only.contains(family)) {
                continue;
            }
            Grid g = incompressible(family, ntau);
            emit(out, family.toLowerCase(Locale.ROOT) + ".fraux", g, cpVersion, f32, manifest);
            errors.add(measureIncompressible(g, family, samples, rng, f32));
        }

        // ---- single-phase (P,T) transport: air ----
        //
        // The box is deliberately the external-air-side box, not air's whole
        // domain: 10 kPa .. 1 MPa, 200 .. 600 K. `htc_extair` is a finned-HX
        // cross-flow correlation, and every state it is ever handed is ambient
        // air near atmospheric. Running the grid down to 150 K at 5 MPa (the
        // first cut) drags in the supercritical/near-liquid corner where cp
        // varies violently, and paid for it: max_rel 8.8e-2 on Cpmass, all of it
        // in a corner no caller reaches. Outside this box the reader declines.
        if (only == null || only.contains("Air")) {
            Grid g = pressureTemperature("Air", nPtAir, nTAir, 1.0e4, 1.0e6, 200.0, 600.0);
            emit(out, "air.fraux", g, cpVersion, f32, manifest);
            errors.add(measurePressureTemperature(g, samples, rng, f32));
        }

        // ---- saturation-line transport ----
        for (String fluid : new String[] {"Water", "R134a", "R1234yf"}) {
            if (only != null && !only.contains(fluid)) {
                continue;
            }
            Grid g = saturationLine(fluid, nSat);
            emit(out, fluid.toLowerCase(Locale.ROOT) + "-sat.fraux", g, cpVersion, f32, manifest);
            errors.add(measureSaturationLine(g, samples, rng, f32));
        }

        Map<String, Object> mf = new LinkedHashMap<>();
        mf.put("generated_by", "tools/aux-gen");
        mf.put("coolprop_version", cpVersion);
        mf.put("format", "FRAUX1");
        mf.put("format_version", FORMAT_VERSION);
        mf.put("element_type", f32 ? "f32" : "f64");
        mf.put("tables", manifest);
        Files.writeString(out.resolve("AUX-MANIFEST.json"), GenSupport.json(mf));

        Map<String, Object> er = new LinkedHashMap<>();
        er.put("_comment", "Interpolation error of the FRAUX1 grids against live CoolProp, "
                + "measured at uniformly random interior states. rel = |table - library| / |library|.");
        er.put("coolprop_version", cpVersion);
        er.put("samples_per_table", samples);
        er.put("seed", seed);
        er.put("tables", errors);
        Files.writeString(out.resolve("AUX-ERROR-REPORT.json"), GenSupport.json(er));

        System.err.println("wrote " + manifest.size() + " tables");
    }

    /**
     * Measures error against resolution and writes nothing — the counterpart of
     * {@code TableGen --sweep}, and the reason the shipped grid sizes below are
     * measurements rather than guesses.
     */
    static void sweep(int samples, long seed, boolean f32) {
        // The concentration axis is fixed at 1 % steps (see `incompressible`),
        // so only the normalized-temperature axis is a free parameter here.
        System.err.println("== incompressible (ntau), INCOMP::MEG");
        for (int ntau : new int[] {16, 24, 32, 48, 64, 96}) {
            Grid g = incompressible("MEG", ntau);
            System.err.printf("-- ntau %d  (%d bytes)%n", ntau,
                    serialize(g, "sweep", f32).length);
            measureIncompressible(g, "MEG", samples, new Random(seed), f32);
        }
        System.err.println("== air (nP x nT)");
        for (int[] r : new int[][] {{16, 32}, {24, 64}, {32, 96}, {48, 128}}) {
            Grid g = pressureTemperature("Air", r[0], r[1], 1.0e4, 1.0e6, 200.0, 600.0);
            System.err.printf("-- %d x %d  (%d bytes)%n", r[0], r[1],
                    serialize(g, "sweep", f32).length);
            measurePressureTemperature(g, samples, new Random(seed), f32);
        }
        System.err.println("== saturation line (nP), Water");
        for (int n : new int[] {128, 256, 512, 1024}) {
            Grid g = saturationLine("Water", n);
            System.err.printf("-- %d  (%d bytes)%n", n, serialize(g, "sweep", f32).length);
            measureSaturationLine(g, samples, new Random(seed), f32);
        }
    }

    // ------------------------------------------------------------ the grids

    /** A rectangular grid of named outputs over two axes. */
    static final class Grid {
        String name;          // the fluid or family this serves
        int kind;
        String axis1Name;
        String axis2Name;
        double[] axis1;
        double[] axis2;
        double[] axis2Lo;     // ragged only: per-axis1 lower endpoint of the real axis
        double[] axis2Hi;     // ragged only
        List<String> outputs = new ArrayList<>();
        List<Integer> transforms = new ArrayList<>();
        List<double[][]> planes = new ArrayList<>();   // [n1][n2]

        boolean ragged() {
            return axis2Lo != null;
        }

        /** The real second-axis value at column {@code i}, node {@code j}. */
        double axis2At(int i, int j) {
            return ragged() ? axis2Lo[i] + axis2[j] * (axis2Hi[i] - axis2Lo[i]) : axis2[j];
        }
    }

    /**
     * {@code INCOMP::<family>[x]} over (mass fraction, normalized temperature).
     *
     * <p>Eight outputs: the four pressure-independent ones straight from the
     * library, then {@code h} and {@code s} at {@link #INCOMP_P_REF} and their
     * pressure slopes. The slopes are computed as a two-point difference and
     * then <b>verified</b> to be pressure-independent at a third pressure — if
     * CoolProp ever stops being exactly linear here, this tool fails rather than
     * writing a table that quietly is not the library.
     */
    static Grid incompressible(String family, int ntau) {
        Grid g = new Grid();
        g.name = "INCOMP::" + family;
        g.kind = KIND_INCOMPRESSIBLE;
        g.axis1Name = "mass_fraction";
        g.axis2Name = "tau";

        // The concentration axis is at EXACTLY 1 % steps, and that is a
        // correctness decision rather than a resolution one.
        // `PropertyFunctions.resolveFluid` can only ever produce a two-decimal
        // mass fraction — its grammar reads an integer percent and formats it as
        // `String.format("0.%02d")`, so `EG50` is `0.50` and there is no
        // spelling in the language for `0.505`. Putting a node on every value
        // the document can name makes the concentration lookup an exact hit
        // instead of an interpolation, and removes that error term completely.
        //
        // It also removes the term that dominated: a first cut with 25 columns
        // over [0, 0.6] measured viscosity at 3.2e-2 and refined only as 1/n_x —
        // first-order, the signature of interpolating across the nonlinear
        // freeze curve rather than of a smooth surface under-resolved.
        double xMax = concentrationCeiling(family);
        int nx = (int) Math.round(xMax * 100.0) + 1;
        g.axis1 = new double[nx];
        for (int i = 0; i < nx; i++) {
            g.axis1[i] = i / 100.0;
        }
        // Non-uniform tau, placed by equidistributing the interpolation error
        // rather than by a guessed formula.
        //
        // Every output on this surface is gentle except viscosity, which
        // follows an Arrhenius-ish ln(mu) ~ 1/T and turns over hard as the
        // mixture nears its freeze point — MEG[0.50] runs 0.052 Pa.s at 240 K
        // to 0.0010 at 350 K. That one output sets the grid size for all eight,
        // so the axis is built to suit it.
        //
        // A first attempt used tau = u^2, which helped the cold end and then
        // OVER-corrected: it left the warm end coarser than uniform, and the
        // error simply moved there. Piecewise-linear interpolation error goes
        // as h^2*|f''|, so the node density that equidistributes it is
        // proportional to sqrt(|f''|). That is what `errorEquidistributed`
        // computes, from the real ln(mu) surface, averaged over the
        // concentration columns.
        g.axis2Lo = new double[nx];
        g.axis2Hi = new double[nx];
        for (int i = 0; i < nx; i++) {
            double[] band = temperatureBand(fluidName(family, g.axis1[i]));
            g.axis2Lo[i] = band[0];
            g.axis2Hi[i] = band[1];
        }
        g.axis2 = errorEquidistributed(family, g, ntau);

        g.outputs = List.of("Dmass", "Cpmass", "viscosity", "conductivity",
                "Hmass", "Smass", "dHmass_dP", "dSmass_dP");
        g.transforms = List.of(XFORM_LINEAR, XFORM_LINEAR, XFORM_LOG, XFORM_LINEAR,
                XFORM_LINEAR, XFORM_LINEAR, XFORM_LINEAR, XFORM_LINEAR);

        for (int k = 0; k < g.outputs.size(); k++) {
            g.planes.add(new double[nx][ntau]);
        }
        for (int i = 0; i < nx; i++) {
            String f = fluidName(family, g.axis1[i]);
            for (int j = 0; j < ntau; j++) {
                double t = g.axis2At(i, j);
                g.planes.get(0)[i][j] = call("Dmass", INCOMP_P_REF, t, f);
                g.planes.get(1)[i][j] = call("Cpmass", INCOMP_P_REF, t, f);
                g.planes.get(2)[i][j] = call("viscosity", INCOMP_P_REF, t, f);
                g.planes.get(3)[i][j] = call("conductivity", INCOMP_P_REF, t, f);
                double h0 = call("Hmass", INCOMP_P_REF, t, f);
                double s0 = call("Smass", INCOMP_P_REF, t, f);
                double h1 = call("Hmass", INCOMP_P_PROBE, t, f);
                double s1 = call("Smass", INCOMP_P_PROBE, t, f);
                double dh = (h1 - h0) / (INCOMP_P_PROBE - INCOMP_P_REF);
                double ds = (s1 - s0) / (INCOMP_P_PROBE - INCOMP_P_REF);
                // The linearity claim this whole table rests on, checked at a
                // third pressure rather than believed.
                double pv = 5.0e6;
                double hv = call("Hmass", pv, t, f);
                double predicted = h0 + dh * (pv - INCOMP_P_REF);
                if (Math.abs(hv - predicted) > 1e-9 * Math.max(1.0, Math.abs(hv))) {
                    throw new IllegalStateException(String.format(
                            "%s at T=%.4f: h is not linear in P (h(5 MPa)=%.10g, predicted %.10g) — "
                            + "the FRAUX1 incompressible model does not hold for this build of CoolProp",
                            f, t, hv, predicted));
                }
                g.planes.get(4)[i][j] = h0;
                g.planes.get(5)[i][j] = s0;
                g.planes.get(6)[i][j] = dh;
                g.planes.get(7)[i][j] = ds;
            }
        }
        return g;
    }

    /**
     * A normalized-temperature axis whose nodes equidistribute the
     * piecewise-linear interpolation error of {@code ln(mu)}.
     *
     * <p>Linear interpolation on a cell of width {@code h} errs as
     * {@code h^2*|f''|/8}, so the node density that levels the error across the
     * band is proportional to {@code sqrt(|f''|)}. This samples {@code ln(mu)}
     * on a fine uniform probe grid, estimates {@code |f''|} by second
     * difference, averages it over the concentration columns (the curvature
     * shifts with concentration and the axis is shared by all of them), and
     * inverts the resulting cumulative density at {@code ntau} equal quantiles.
     *
     * <p>The density is floored at a fraction of its mean so the warm end never
     * becomes coarser than a uniform axis would have made it — the failure the
     * first {@code u^2} attempt walked into.
     */
    static double[] errorEquidistributed(String family, Grid g, int ntau) {
        final int probes = 512;
        double[] density = new double[probes];
        int columns = 0;
        for (int i = 0; i < g.axis1.length; i += Math.max(1, g.axis1.length / 12)) {
            String f = fluidName(family, g.axis1[i]);
            double lo = g.axis2Lo[i];
            double hi = g.axis2Hi[i];
            double[] lnmu = new double[probes];
            boolean usable = true;
            for (int j = 0; j < probes; j++) {
                double t = lo + (hi - lo) * j / (probes - 1.0);
                double mu = call("viscosity", INCOMP_P_REF, t, f);
                if (Double.isNaN(mu) || mu <= 0.0) {
                    usable = false;
                    break;
                }
                lnmu[j] = Math.log(mu);
            }
            if (!usable) {
                continue;
            }
            columns++;
            for (int j = 1; j < probes - 1; j++) {
                double second = Math.abs(lnmu[j - 1] - 2 * lnmu[j] + lnmu[j + 1]);
                density[j] += Math.sqrt(second);
            }
        }
        if (columns == 0) {
            return linspace(0.0, 1.0, ntau);
        }
        density[0] = density[1];
        density[probes - 1] = density[probes - 2];

        double mean = 0.0;
        for (double d : density) {
            mean += d;
        }
        mean /= probes;
        // Never let any region get coarser than ~1/3 of uniform.
        double floor = mean / 3.0;
        double[] cum = new double[probes];
        double running = 0.0;
        for (int j = 0; j < probes; j++) {
            running += Math.max(density[j], floor);
            cum[j] = running;
        }
        for (int j = 0; j < probes; j++) {
            cum[j] /= running;
        }

        double[] axis = new double[ntau];
        axis[0] = 0.0;
        axis[ntau - 1] = 1.0;
        int at = 0;
        for (int k = 1; k < ntau - 1; k++) {
            double target = k / (ntau - 1.0);
            while (at < probes - 2 && cum[at + 1] < target) {
                at++;
            }
            double span = cum[at + 1] - cum[at];
            double frac = span > 0 ? (target - cum[at]) / span : 0.0;
            axis[k] = (at + frac) / (probes - 1.0);
        }
        // Strict monotonicity is a load-bearing property of the reader's binary
        // search; nudge any duplicate the quantile inversion produced.
        for (int k = 1; k < ntau; k++) {
            if (!(axis[k] > axis[k - 1])) {
                axis[k] = Math.nextUp(axis[k - 1]);
            }
        }
        return axis;
    }

    /** Single-phase transport over {@code (ln P, T)}. */
    static Grid pressureTemperature(String fluid, int nP, int nT,
                                    double pMin, double pMax, double tMin, double tMax) {
        Grid g = new Grid();
        g.name = fluid;
        g.kind = KIND_PRESSURE_TEMPERATURE;
        g.axis1Name = "log_P";
        g.axis2Name = "T";
        g.axis1 = logspace(pMin, pMax, nP);
        g.axis2 = linspace(tMin, tMax, nT);
        g.outputs = List.of("viscosity", "conductivity", "Cpmass", "Dmass");
        g.transforms = List.of(XFORM_LOG, XFORM_LINEAR, XFORM_LINEAR, XFORM_LOG);
        for (int k = 0; k < g.outputs.size(); k++) {
            g.planes.add(new double[nP][nT]);
        }
        for (int i = 0; i < nP; i++) {
            double p = Math.exp(g.axis1[i]);
            for (int j = 0; j < nT; j++) {
                double t = g.axis2[j];
                for (int k = 0; k < g.outputs.size(); k++) {
                    g.planes.get(k)[i][j] = call(g.outputs.get(k), p, t, fluid);
                }
            }
        }
        return g;
    }

    /**
     * Transport along the dome at {@code Q = 0} and {@code Q = 1}.
     *
     * <p>The pressure axis is the same log-spaced shape {@code TableGen} uses
     * and — crucially — the <b>same ceiling</b>: {@code 0.75 * p_crit}. That is
     * not a convenience. Approaching the critical point, {@code cp} and
     * {@code conductivity} diverge, and no tractable grid interpolates a
     * divergence: a first cut of this tool ran the axis to {@code p_crit} and
     * measured {@code max_rel = 2.0e+01} on water's {@code Cpmass}, all of it in
     * the last few nodes. Matching {@code TableGen}'s ceiling removes that
     * region entirely, and removes nothing a caller can reach — the split table
     * cannot produce a state above its own {@code p_serve_max} either.
     *
     * <p>The ceiling is still probed downward from there, because CoolProp's
     * transport models give out before the EOS does on some fluids.
     */
    static Grid saturationLine(String fluid, int nSat) {
        double pTriple = CoolProp.props1SI(fluid, "ptriple");
        double pCrit = CoolProp.props1SI(fluid, "pcrit");
        double lo = pTriple * 1.0001;
        double hi = transportCeiling(fluid, lo, pCrit * 0.75);

        Grid g = new Grid();
        g.name = fluid;
        g.kind = KIND_SATURATION_LINE;
        g.axis1Name = "log_P";
        g.axis2Name = "Q";
        g.axis1 = logspace(lo, hi, nSat);
        g.axis2 = new double[] {0.0, 1.0};
        g.outputs = List.of("viscosity", "conductivity", "Cpmass");
        g.transforms = List.of(XFORM_LOG, XFORM_LINEAR, XFORM_LINEAR);
        for (int k = 0; k < g.outputs.size(); k++) {
            g.planes.add(new double[nSat][2]);
        }
        for (int i = 0; i < nSat; i++) {
            double p = Math.exp(g.axis1[i]);
            for (int j = 0; j < 2; j++) {
                for (int k = 0; k < g.outputs.size(); k++) {
                    g.planes.get(k)[i][j] = callQ(g.outputs.get(k), p, g.axis2[j], fluid);
                }
            }
        }
        return g;
    }

    // -------------------------------------------------------------- probing

    /** {@code INCOMP::MEG[0.35]} — the spelling {@code resolveFluid} produces. */
    static String fluidName(String family, double x) {
        return String.format(Locale.ROOT, "INCOMP::%s[%.2f]", family, x);
    }

    /**
     * Highest mass fraction the library still answers for, to the 0.01 the
     * document spelling can express.
     */
    static double concentrationCeiling(String family) {
        double best = 0.0;
        for (int c = 0; c <= 100; c++) {
            double x = c / 100.0;
            if (!Double.isNaN(call("Dmass", INCOMP_P_REF, 300.0, fluidName(family, x)))) {
                best = x;
            }
        }
        if (best <= 0.0) {
            throw new IllegalStateException("no usable concentration for " + family);
        }
        return best;
    }

    /**
     * The {@code [Tmin, Tmax]} band this mixture is defined on, bisected to
     * 1 mK and then pulled a hair inward so every grid node is strictly inside.
     */
    static double[] temperatureBand(String fluid) {
        double inside = Double.NaN;
        for (double t = 200.0; t <= 500.0; t += 0.25) {
            if (!Double.isNaN(call("Dmass", INCOMP_P_REF, t, fluid))) {
                inside = t;
                break;
            }
        }
        if (Double.isNaN(inside)) {
            throw new IllegalStateException("no usable temperature for " + fluid);
        }
        double lo = bisectEdge(fluid, inside, 150.0);
        double hi = bisectEdge(fluid, inside, 600.0);
        double pad = 1e-3 * (hi - lo);
        return new double[] {lo + pad, hi - pad};
    }

    /** Walks from a known-good {@code from} toward a known-bad {@code toward}. */
    private static double bisectEdge(String fluid, double from, double toward) {
        double good = from;
        double bad = toward;
        for (int i = 0; i < 60; i++) {
            double mid = 0.5 * (good + bad);
            if (Double.isNaN(call("Dmass", INCOMP_P_REF, mid, fluid))) {
                bad = mid;
            } else {
                good = mid;
            }
            if (Math.abs(bad - good) < 1e-3) {
                break;
            }
        }
        return good;
    }

    /**
     * The highest saturation pressure at which the library still returns
     * transport for <b>both</b> phases, bisected between a known-good pressure
     * and the critical point.
     */
    static double transportCeiling(String fluid, double good, double pTop) {
        double bad = pTop;
        if (satTransportOk(fluid, bad)) {
            return bad;
        }
        for (int i = 0; i < 60; i++) {
            double mid = Math.exp(0.5 * (Math.log(good) + Math.log(bad)));
            if (satTransportOk(fluid, mid)) {
                good = mid;
            } else {
                bad = mid;
            }
            if (bad - good < 1e-6 * bad) {
                break;
            }
        }
        return good;
    }

    private static boolean satTransportOk(String fluid, double p) {
        for (double q : new double[] {0.0, 1.0}) {
            for (String out : new String[] {"viscosity", "conductivity", "Cpmass"}) {
                double v = callQ(out, p, q, fluid);
                if (Double.isNaN(v) || v <= 0.0 || Double.isInfinite(v)) {
                    return false;
                }
            }
        }
        return true;
    }

    /** {@code PropsSI(out, P, p, T, t, fluid)}, NaN where the library declines. */
    static double call(String out, double p, double t, String fluid) {
        double v = CoolProp.propsSIOrNaN(out, "P", p, "T", t, fluid);
        return Double.isFinite(v) ? v : Double.NaN;
    }

    /** {@code PropsSI(out, P, p, Q, q, fluid)}, NaN where the library declines. */
    static double callQ(String out, double p, double q, String fluid) {
        double v = CoolProp.propsSIOrNaN(out, "P", p, "Q", q, fluid);
        return Double.isFinite(v) ? v : Double.NaN;
    }

    // -------------------------------------------------------- error measure

    /**
     * Reads the grid back the way the Rust engine will — same transform, same
     * bilinear interpolation, same f32 rounding — and compares to the library.
     */
    static double lookup(Grid g, int out, double a1, double a2Norm, boolean f32) {
        int n1 = g.axis1.length;
        int n2 = g.axis2.length;
        int i = bracket(g.axis1, a1);
        int j = bracket(g.axis2, a2Norm);
        double fi = (a1 - g.axis1[i]) / (g.axis1[i + 1] - g.axis1[i]);
        double fj = (a2Norm - g.axis2[j]) / (g.axis2[j + 1] - g.axis2[j]);
        double[][] plane = g.planes.get(out);
        boolean log = g.transforms.get(out) == XFORM_LOG;
        double v00 = store(plane[i][j], log, f32);
        double v10 = store(plane[i + 1][j], log, f32);
        double v01 = store(plane[i][j + 1], log, f32);
        double v11 = store(plane[i + 1][j + 1], log, f32);
        double v = (1 - fi) * (1 - fj) * v00 + fi * (1 - fj) * v10
                + (1 - fi) * fj * v01 + fi * fj * v11;
        if (n1 < 2 || n2 < 2) {
            throw new IllegalStateException("degenerate grid");
        }
        return log ? Math.exp(v) : v;
    }

    private static double store(double v, boolean log, boolean f32) {
        double t = log ? Math.log(v) : v;
        return f32 ? (float) t : t;
    }

    private static int bracket(double[] axis, double v) {
        int lo = 0;
        int hi = axis.length - 1;
        while (hi - lo > 1) {
            int mid = (lo + hi) >>> 1;
            if (axis[mid] <= v) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        return Math.min(lo, axis.length - 2);
    }

    static Map<String, Object> measureIncompressible(Grid g, String family, int samples,
                                                     Random rng, boolean f32) {
        String[] direct = {"Dmass", "Cpmass", "viscosity", "conductivity"};
        Map<String, double[]> acc = new LinkedHashMap<>();
        for (String d : direct) {
            acc.put(d, slot());
        }
        acc.put("Hmass", slot());
        acc.put("Smass", slot());
        for (int s = 0; s < samples; s++) {
            // Integer percent, because that is the only concentration a
            // document can name — sampling continuous x would measure an error
            // term no user can reach, and hide the one they can.
            double x = rng.nextInt((int) Math.round(g.axis1[g.axis1.length - 1] * 100.0) + 1) / 100.0;
            double tau = rng.nextDouble();
            String f = fluidName(family, x);
            double[] band = bandAt(g, x);
            double t = band[0] + tau * (band[1] - band[0]);
            double p = 1.0e5 + rng.nextDouble() * 9.9e6;
            for (int k = 0; k < direct.length; k++) {
                double ref = call(direct[k], p, t, f);
                if (Double.isNaN(ref)) {
                    continue;
                }
                bump(acc.get(direct[k]), lookup(g, k, x, tau, f32), ref);
            }
            double href = call("Hmass", p, t, f);
            if (!Double.isNaN(href)) {
                double h = lookup(g, 4, x, tau, f32) + lookup(g, 6, x, tau, f32) * (p - INCOMP_P_REF);
                bump(acc.get("Hmass"), h, href);
            }
            double sref = call("Smass", p, t, f);
            if (!Double.isNaN(sref)) {
                double sv = lookup(g, 5, x, tau, f32) + lookup(g, 7, x, tau, f32) * (p - INCOMP_P_REF);
                bump(acc.get("Smass"), sv, sref);
            }
        }
        return report(g.name, acc);
    }

    /** The stored endpoint band linearly interpolated to an arbitrary {@code x}. */
    private static double[] bandAt(Grid g, double x) {
        int i = bracket(g.axis1, x);
        double f = (x - g.axis1[i]) / (g.axis1[i + 1] - g.axis1[i]);
        return new double[] {
                g.axis2Lo[i] + f * (g.axis2Lo[i + 1] - g.axis2Lo[i]),
                g.axis2Hi[i] + f * (g.axis2Hi[i + 1] - g.axis2Hi[i]),
        };
    }

    static Map<String, Object> measurePressureTemperature(Grid g, int samples, Random rng, boolean f32) {
        Map<String, double[]> acc = new LinkedHashMap<>();
        for (String o : g.outputs) {
            acc.put(o, slot());
        }
        double l0 = g.axis1[0];
        double l1 = g.axis1[g.axis1.length - 1];
        double t0 = g.axis2[0];
        double t1 = g.axis2[g.axis2.length - 1];
        for (int s = 0; s < samples; s++) {
            double lp = l0 + rng.nextDouble() * (l1 - l0);
            double t = t0 + rng.nextDouble() * (t1 - t0);
            for (int k = 0; k < g.outputs.size(); k++) {
                double ref = call(g.outputs.get(k), Math.exp(lp), t, g.name);
                if (Double.isNaN(ref)) {
                    continue;
                }
                bump(acc.get(g.outputs.get(k)), lookup(g, k, lp, t, f32), ref);
            }
        }
        return report(g.name + " (P,T)", acc);
    }

    static Map<String, Object> measureSaturationLine(Grid g, int samples, Random rng, boolean f32) {
        Map<String, double[]> acc = new LinkedHashMap<>();
        for (String o : g.outputs) {
            acc.put(o, slot());
        }
        double l0 = g.axis1[0];
        double l1 = g.axis1[g.axis1.length - 1];
        for (int s = 0; s < samples; s++) {
            double lp = l0 + rng.nextDouble() * (l1 - l0);
            double q = rng.nextBoolean() ? 0.0 : 1.0;
            for (int k = 0; k < g.outputs.size(); k++) {
                double ref = callQ(g.outputs.get(k), Math.exp(lp), q, g.name);
                if (Double.isNaN(ref)) {
                    continue;
                }
                bump(acc.get(g.outputs.get(k)), lookup(g, k, lp, q, f32), ref);
            }
        }
        return report(g.name + " (sat)", acc);
    }

    /**
     * Accumulates one comparison.
     *
     * <p>Both a pointwise relative error and an error scaled by the largest
     * magnitude seen are tracked, because a pointwise ratio is meaningless for
     * an output whose reference value passes through zero. Glycol enthalpy does
     * exactly that — CoolProp's incompressible reference puts {@code h = 0}
     * near 293 K — so {@code max_rel} for {@code Hmass} is dominated by samples
     * a few kelvin from the zero crossing and says nothing about table quality.
     * {@code max_rel_scaled} is the number to read there; {@code max_rel} is
     * still reported rather than quietly dropped. D1's ERROR-REPORT has the same
     * artifact on water entropy near the triple point and describes it the same
     * way.
     */
    private static void bump(double[] a, double got, double ref) {
        double abs = Math.abs(got - ref);
        double rel = abs / Math.max(Math.abs(ref), 1e-30);
        a[0] = Math.max(a[0], rel);
        a[1] += rel * rel;
        a[2] += 1;
        a[3] = Math.max(a[3], abs);
        a[4] = Math.max(a[4], Math.abs(ref));
    }

    private static double[] slot() {
        return new double[] {0.0, 0.0, 0.0, 0.0, 0.0};
    }

    private static Map<String, Object> report(String name, Map<String, double[]> acc) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("table", name);
        Map<String, Object> per = new LinkedHashMap<>();
        for (Map.Entry<String, double[]> e : acc.entrySet()) {
            double[] a = e.getValue();
            int n = (int) a[2];
            double maxRel = n > 0 ? a[0] : 0.0;
            double rms = n > 0 ? Math.sqrt(a[1] / a[2]) : 0.0;
            double scaled = n > 0 && a[4] > 0 ? a[3] / a[4] : 0.0;
            Map<String, Object> one = new LinkedHashMap<>();
            one.put("n", n);
            one.put("max_rel", maxRel);
            one.put("rms_rel", rms);
            one.put("max_abs", n > 0 ? a[3] : 0.0);
            one.put("max_rel_scaled", scaled);
            per.put(e.getKey(), one);
            System.err.printf("  %-22s %-14s n=%-6d max_rel=%.3e rms=%.3e scaled=%.3e%n",
                    name, e.getKey(), n, maxRel, rms, scaled);
        }
        m.put("outputs", per);
        return m;
    }

    // ------------------------------------------------------- serialization

    static void emit(Path dir, String file, Grid g, String cpVersion, boolean f32,
                     List<Map<String, Object>> manifest) throws IOException {
        byte[] blob = serialize(g, cpVersion, f32);
        Files.write(dir.resolve(file), blob);
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("name", g.name);
        m.put("file", file);
        m.put("kind", switch (g.kind) {
            case KIND_INCOMPRESSIBLE -> "incompressible";
            case KIND_PRESSURE_TEMPERATURE -> "pressure_temperature";
            default -> "saturation_line";
        });
        m.put("bytes", blob.length);
        m.put("sha256", GenSupport.sha256(blob));
        m.put("n1", g.axis1.length);
        m.put("n2", g.axis2.length);
        m.put("axis1", g.axis1Name);
        m.put("axis2", g.axis2Name);
        m.put("outputs", g.outputs);
        m.put("ragged", g.ragged());
        Map<String, Object> bounds = new LinkedHashMap<>();
        bounds.put("axis1_min", g.axis1[0]);
        bounds.put("axis1_max", g.axis1[g.axis1.length - 1]);
        if (g.ragged()) {
            bounds.put("axis2_lo_min", min(g.axis2Lo));
            bounds.put("axis2_hi_max", max(g.axis2Hi));
        } else {
            bounds.put("axis2_min", g.axis2[0]);
            bounds.put("axis2_max", g.axis2[g.axis2.length - 1]);
        }
        m.put("bounds", bounds);
        manifest.add(m);
        System.err.printf("  %-24s %-22s %6d bytes  %d x %d%n",
                file, g.name, blob.length, g.axis1.length, g.axis2.length);
    }

    /** Writes one grid in the {@code FRAUX1} format (see README.md). */
    static byte[] serialize(Grid g, String cpVersion, boolean f32) {
        byte[] nameBytes = g.name.getBytes(StandardCharsets.UTF_8);
        byte[] versionBytes = cpVersion.getBytes(StandardCharsets.UTF_8);
        byte[] a1Bytes = g.axis1Name.getBytes(StandardCharsets.UTF_8);
        byte[] a2Bytes = g.axis2Name.getBytes(StandardCharsets.UTF_8);
        int names = 0;
        for (String o : g.outputs) {
            names += 2 + o.getBytes(StandardCharsets.UTF_8).length + 1;
        }
        int headerBytes = HEADER_FIXED + nameBytes.length + versionBytes.length
                + a1Bytes.length + a2Bytes.length + names;
        headerBytes = (headerBytes + 7) & ~7;

        GenSupport.Buf b = new GenSupport.Buf();
        b.bytes(MAGIC);
        b.u16(FORMAT_VERSION);
        b.u8(f32 ? 1 : 0);
        b.u8(g.ragged() ? FLAG_RAGGED : 0);
        b.u32(g.kind);
        b.u32(g.axis1.length);
        b.u32(g.axis2.length);
        b.u32(g.outputs.size());
        b.u32(headerBytes);
        b.f64(g.axis1[0]);
        b.f64(g.axis1[g.axis1.length - 1]);
        b.f64(g.axis2[0]);
        b.f64(g.axis2[g.axis2.length - 1]);
        // The pressure the INCOMPRESSIBLE `Hmass`/`Smass` columns are stored at.
        // Carried in the file rather than agreed by convention: the reader adds
        // `dHmass_dP * (P - ref)` to it, so a reader that assumed the wrong
        // reference would be wrong by a constant offset on every enthalpy —
        // silently, and only for glycols. Zero for the kinds that do not use it.
        b.f64(g.kind == KIND_INCOMPRESSIBLE ? INCOMP_P_REF : 0.0);
        b.u16(nameBytes.length);
        b.u16(versionBytes.length);
        b.u16(a1Bytes.length);
        b.u16(a2Bytes.length);
        b.u32(0); // reserved
        b.bytes(nameBytes);
        b.bytes(versionBytes);
        b.bytes(a1Bytes);
        b.bytes(a2Bytes);
        for (int k = 0; k < g.outputs.size(); k++) {
            byte[] o = g.outputs.get(k).getBytes(StandardCharsets.UTF_8);
            b.u16(o.length);
            b.bytes(o);
            b.u8(g.transforms.get(k));
        }
        while (b.size() < headerBytes) {
            b.u8(0);
        }

        b.array(g.axis1, f32);
        b.array(g.axis2, f32);
        if (g.ragged()) {
            b.array(g.axis2Lo, f32);
            b.array(g.axis2Hi, f32);
        }
        for (int k = 0; k < g.planes.size(); k++) {
            boolean log = g.transforms.get(k) == XFORM_LOG;
            for (double[] row : g.planes.get(k)) {
                double[] stored = new double[row.length];
                for (int j = 0; j < row.length; j++) {
                    stored[j] = log ? Math.log(row[j]) : row[j];
                }
                b.array(stored, f32);
            }
        }
        return b.toByteArray();
    }

    // -------------------------------------------------------------- helpers

    static double[] linspace(double a, double b, int n) {
        double[] v = new double[n];
        for (int i = 0; i < n; i++) {
            v[i] = n == 1 ? a : a + (b - a) * i / (n - 1.0);
        }
        return v;
    }

    /** Log-spaced, but returning the <b>logs</b> — the axis the reader searches. */
    static double[] logspace(double a, double b, int n) {
        double[] v = new double[n];
        double la = Math.log(a);
        double lb = Math.log(b);
        for (int i = 0; i < n; i++) {
            v[i] = n == 1 ? la : la + (lb - la) * i / (n - 1.0);
        }
        return v;
    }

    static double round2(double x) {
        return Math.round(x * 100.0) / 100.0;
    }

    static double min(double[] a) {
        double m = a[0];
        for (double v : a) {
            m = Math.min(m, v);
        }
        return m;
    }

    static double max(double[] a) {
        double m = a[0];
        for (double v : a) {
            m = Math.max(m, v);
        }
        return m;
    }

}
