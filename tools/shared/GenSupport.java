package com.frees.backend.props;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.List;
import java.util.Map;

/**
 * The plumbing {@code TableGen} and {@code AuxGen} both need, in one place.
 *
 * <p>Both tools write a little-endian binary artifact plus a JSON manifest with
 * a SHA-256, and both have to ask the native library its version through a
 * binding the engine's {@link CoolProp} façade does not expose. That is four
 * pieces of infrastructure with no reason to differ between them — and every
 * reason not to, since a divergence in the byte sink or the digest would show
 * up as two artifacts that disagree about their own checksums.
 *
 * <p>It is compiled into each tool's own build directory by that tool's
 * {@code run.sh}; there is no jar and no build system, which is deliberate
 * (see {@code TableGen}'s class comment on adding no dependency the reference
 * engine does not already carry).
 */
public final class GenSupport {

    private GenSupport() {
    }

    // ------------------------------------------------------------ byte sink

    /**
     * Little-endian byte sink. Everything both formats write goes through here,
     * so "little-endian, unpadded" is asserted once rather than per tool.
     */
    public static final class Buf {
        private final ByteArrayOutputStream out = new ByteArrayOutputStream(1 << 20);

        public void u8(int v) {
            out.write(v & 0xFF);
        }

        public void u16(int v) {
            u8(v);
            u8(v >>> 8);
        }

        public void u32(int v) {
            u16(v);
            u16(v >>> 16);
        }

        public void f64(double v) {
            long bits = Double.doubleToLongBits(v);
            u32((int) bits);
            u32((int) (bits >>> 32));
        }

        public void f32(double v) {
            u32(Float.floatToIntBits((float) v));
        }

        public void array(double[] a, boolean asF32) {
            for (double v : a) {
                if (asF32) {
                    f32(v);
                } else {
                    f64(v);
                }
            }
        }

        public void bytes(byte[] a) {
            out.write(a, 0, a.length);
        }

        public int size() {
            return out.size();
        }

        public byte[] toByteArray() {
            return out.toByteArray();
        }
    }

    // --------------------------------------------------------------- digest

    /** Lower-case hex SHA-256, the form both manifests record. */
    public static String sha256(byte[] data) {
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256").digest(data);
            StringBuilder sb = new StringBuilder(64);
            for (byte v : digest) {
                sb.append(String.format("%02x", v));
            }
            return sb.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    // ----------------------------------------------------- CoolProp version

    /**
     * {@code get_global_param_string("version")}.
     *
     * <p>The engine's {@link CoolProp} façade makes this call for its own error
     * string but does not expose it, and its {@code Lib} interface is private —
     * so this declares the one binding it needs. JNA is already a core
     * dependency; no new jar enters the classpath.
     */
    public static String coolPropVersion() {
        interface Lib extends com.sun.jna.Library {
            int get_global_param_string(String param, byte[] output, int n);
        }
        String path = System.getenv("COOLPROP_LIBRARY");
        Lib lib = com.sun.jna.Native.load(
                path != null && !path.isBlank() ? path : "CoolProp", Lib.class);
        byte[] buffer = new byte[256];
        lib.get_global_param_string("version", buffer, buffer.length);
        String version = com.sun.jna.Native.toString(buffer);
        return version.isBlank() ? "unknown" : version;
    }

    // ------------------------------------------------------- tiny JSON out

    /** Hand-rolled JSON, same reason as {@code GoldenDumper}: no Jackson on this classpath. */
    public static String json(Object value) {
        StringBuilder sb = new StringBuilder();
        write(sb, value, 0);
        sb.append('\n');
        return sb.toString();
    }

    private static void write(StringBuilder sb, Object value, int indent) {
        String pad = "  ".repeat(indent + 1);
        String padEnd = "  ".repeat(indent);
        switch (value) {
            case null -> sb.append("null");
            case Map<?, ?> map -> {
                if (map.isEmpty()) {
                    sb.append("{}");
                    return;
                }
                sb.append("{\n");
                int i = 0;
                for (Map.Entry<?, ?> e : map.entrySet()) {
                    sb.append(pad).append(quote(String.valueOf(e.getKey()))).append(": ");
                    write(sb, e.getValue(), indent + 1);
                    sb.append(++i < map.size() ? ",\n" : "\n");
                }
                sb.append(padEnd).append('}');
            }
            case List<?> list -> {
                if (list.isEmpty()) {
                    sb.append("[]");
                    return;
                }
                sb.append("[\n");
                for (int i = 0; i < list.size(); i++) {
                    sb.append(pad);
                    write(sb, list.get(i), indent + 1);
                    sb.append(i + 1 < list.size() ? ",\n" : "\n");
                }
                sb.append(padEnd).append(']');
            }
            case Double d -> sb.append(d.isNaN() || d.isInfinite() ? quote(d.toString()) : d.toString());
            case Number n -> sb.append(n);
            case Boolean b -> sb.append(b);
            default -> sb.append(quote(String.valueOf(value)));
        }
    }

    private static String quote(String s) {
        StringBuilder out = new StringBuilder(s.length() + 16).append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> {
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
                }
            }
        }
        return out.append('"').toString();
    }

    /** UTF-8 bytes, the encoding both formats use for every embedded string. */
    public static byte[] utf8(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }
}
