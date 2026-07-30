#!/usr/bin/env python3
"""Reference reader for the FRPHTAB1 format — stdlib only, no dependencies.

Two jobs:

  1. Prove that README.md's format table describes the bytes TableGen actually
     writes. A spec nobody executes is a spec that drifts.
  2. Give whoever ports the reader to Rust something to diff against: this
     parses a `.phtab` in ~100 lines with no cleverness, so a disagreement
     points at the Rust reader rather than at an ambiguity in the prose.

    ./inspect.py fixtures/proptables/water.phtab
    ./inspect.py fixtures/proptables/*.phtab --nodes      # also dump a corner
"""

import struct
import sys

MAGIC = b"FRPHTAB1"
OUTPUTS = ("T", "Dmass", "Smass")
SAT_LINES = ("log_p", "t_sat", "h_f", "h_g", "v_f", "v_fg", "s_f", "s_fg", "h_cold")


class Reader:
    def __init__(self, blob):
        self.blob = blob
        self.pos = 0

    def take(self, n):
        chunk = self.blob[self.pos:self.pos + n]
        if len(chunk) != n:
            raise ValueError(f"truncated: wanted {n} bytes at {self.pos}")
        self.pos += n
        return chunk

    def scalar(self, fmt):
        return struct.unpack_from(fmt, self.take(struct.calcsize(fmt)))[0]

    def array(self, n, elem):
        fmt = "<" + ("f" if elem == 4 else "d") * n
        return list(struct.unpack_from(fmt, self.take(n * elem)))


def load(path):
    with open(path, "rb") as fh:
        blob = fh.read()
    r = Reader(blob)
    if r.take(8) != MAGIC:
        raise ValueError(f"{path}: not a FRPHTAB file")
    t = {"format_version": r.scalar("<H")}
    if t["format_version"] != 1:
        raise ValueError(f"{path}: unsupported format_version {t['format_version']}")
    elem_kind = r.scalar("<B")
    flags = r.scalar("<B")
    t["element_type"] = "f32" if elem_kind else "f64"
    t["has_liquid"] = bool(flags & 1)
    t["liquid_normalized"] = bool(flags & 2)
    t["n_sat"] = r.scalar("<I")
    t["n_p"] = r.scalar("<I")
    t["n_dh"] = r.scalar("<I")
    t["n_props"] = r.scalar("<I")
    header_bytes = r.scalar("<I")
    for name in ("p_min", "p_max", "p_serve_max", "p_liquid_min", "dh_vapor_max",
                 "dh_liquid_max", "h_top", "t_low", "p_crit", "t_crit",
                 "p_triple", "t_triple"):
        t[name] = r.scalar("<d")
    fluid_len = r.scalar("<H")
    version_len = r.scalar("<H")
    t["backfilled_nodes"] = r.scalar("<I")
    t["fluid"] = r.take(fluid_len).decode("utf-8")
    t["coolprop_version"] = r.take(version_len).decode("utf-8")

    if header_bytes % 8:
        raise ValueError(f"{path}: header_bytes {header_bytes} is not 8-aligned")
    if r.pos > header_bytes:
        raise ValueError(f"{path}: strings overran header_bytes")
    r.pos = header_bytes

    elem = 4 if elem_kind else 8
    n_sat, n_p, n_dh = t["n_sat"], t["n_p"], t["n_dh"]
    t["sat"] = {name: r.array(n_sat, elem) for name in SAT_LINES}

    def piece():
        out = {"p_grid": r.array(n_p, elem), "y_grid": r.array(n_dh, elem)}
        out["planes"] = {name: [r.array(n_dh, elem) for _ in range(n_p)] for name in OUTPUTS}
        return out

    t["vapor"] = piece()
    t["liquid"] = piece() if t["has_liquid"] else None
    if r.pos != len(blob):
        raise ValueError(f"{path}: {len(blob) - r.pos} trailing bytes — layout mismatch")
    t["bytes"] = len(blob)
    return t


def check(t, path):
    """Invariants the README promises. Any violation is a generator bug."""
    problems = []
    if t["n_props"] != len(OUTPUTS):
        problems.append(f"n_props {t['n_props']} != {len(OUTPUTS)}")
    for name, values in t["sat"].items():
        if any(v != v for v in values):
            problems.append(f"sat line {name} contains NaN")
    if t["sat"]["log_p"] != sorted(t["sat"]["log_p"]):
        problems.append("log_p is not increasing")
    for label, piece in (("vapor", t["vapor"]), ("liquid", t["liquid"])):
        if piece is None:
            continue
        if piece["p_grid"] != sorted(piece["p_grid"]):
            problems.append(f"{label} p_grid is not increasing")
        if piece["y_grid"] != sorted(piece["y_grid"]):
            problems.append(f"{label} y_grid is not increasing")
        if piece["y_grid"][0] != 0.0:
            problems.append(f"{label} y_grid[0] = {piece['y_grid'][0]}, expected 0")
        for name, plane in piece["planes"].items():
            bad = sum(1 for row in plane for v in row if v != v or abs(v) == float("inf"))
            if bad:
                problems.append(f"{label}/{name}: {bad} non-finite nodes (back-fill missed them)")
    if not (t["p_min"] < t["p_serve_max"] <= t["p_max"] < t["p_crit"]):
        problems.append("pressure bounds are not ordered p_min < p_serve_max <= p_max < p_crit")
    for p in problems:
        print(f"  FAIL {path}: {p}")
    return not problems


def main(argv):
    show_nodes = "--nodes" in argv
    paths = [a for a in argv[1:] if not a.startswith("--")]
    if not paths:
        print(__doc__)
        return 2
    ok = True
    for path in paths:
        t = load(path)
        print(f"{path}")
        print(f"  fluid {t['fluid']}  CoolProp {t['coolprop_version']}  {t['element_type']}"
              f"  {t['bytes']:,} bytes")
        print(f"  grid  n_sat={t['n_sat']} n_p={t['n_p']} n_dh={t['n_dh']}"
              f"  liquid={'normalized' if t['liquid_normalized'] else 'absolute'}"
              f" {'present' if t['has_liquid'] else 'ABSENT'}"
              f"  back-filled {t['backfilled_nodes']}")
        print(f"  P     [{t['p_min']:.6g}, {t['p_max']:.6g}] Pa, served to {t['p_serve_max']:.6g}"
              f"  (p_crit {t['p_crit']:.6g})")
        print(f"  Tsat  [{t['sat']['t_sat'][0]:.4f}, {t['sat']['t_sat'][-1]:.4f}] K"
              f"  (t_crit {t['t_crit']:.4f}, t_low {t['t_low']:.4f})")
        print(f"  h_f   [{t['sat']['h_f'][0]:.6g}, {t['sat']['h_f'][-1]:.6g}] J/kg")
        print(f"  h_g   [{t['sat']['h_g'][0]:.6g}, {t['sat']['h_g'][-1]:.6g}] J/kg")
        print(f"  vapor y up to {t['dh_vapor_max']:.6g}   liquid y up to {t['dh_liquid_max']:.6g}")
        if show_nodes:
            for label, piece in (("vapor", t["vapor"]), ("liquid", t["liquid"])):
                if piece is None:
                    continue
                for name, plane in piece["planes"].items():
                    print(f"  {label}/{name}[0][0:4] = "
                          + ", ".join(f"{v:.8g}" for v in plane[0][:4]))
        ok &= check(t, path)
    print("OK" if ok else "PROBLEMS FOUND")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
