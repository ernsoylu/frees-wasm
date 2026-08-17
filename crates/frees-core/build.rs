//! Packs the linked property artifacts so they cost fewer bytes in the wasm
//! module than they do on disk.
//!
//! `src/props/data/` holds 1014 KB of `FRPHTAB1` / `FRAUX1` grids that
//! `props/tables.rs` used to `include_bytes!` verbatim. That put all 1014 KB
//! into the wasm data section, which is 40 % of the module and the larger half
//! of the budget breach D7 recorded. Deflating them here and inflating once at
//! install time gets the same bytes for ~685 KB.
//!
//! # Why the shuffle
//!
//! Deflate alone barely moves these files — D7 measured gzip reaching only
//! 0.87–0.94 on them and concluded "compression is not a lever". That
//! measurement is right about *plain* deflate and wrong about the conclusion.
//! The payloads are `f32` grids sampling smooth functions, so consecutive
//! elements differ mostly in their low mantissa bytes while their exponent and
//! sign bytes repeat for thousands of samples running. Interleaved, every
//! 4-byte group looks different and LZ77 finds almost no matches. Transposed
//! into byte planes — all byte-0s, then all byte-1s, and so on, the HDF5
//! "shuffle" filter — the repetitive planes become long runs and the entropy
//! coder gets its teeth into them. Measured over the nine artifacts:
//!
//! ```text
//!   raw               1014 KB
//!   deflate            904 KB   (0.89 — D7's number)
//!   shuffle + deflate  685 KB   (0.68)
//! ```
//!
//! The transform is a pure byte permutation, so it is lossless independently of
//! what the bytes mean: nothing here needs to know the `FRPHTAB1` header layout,
//! and a generator change cannot silently corrupt a table through it. The
//! artifacts in `src/props/data/` and `fixtures/` stay exactly as `tools/aux-gen`
//! and `tools/table-gen` wrote them — this is a container, applied on the way
//! into the binary and undone on the way out.

use std::env;
use std::fs;
use std::path::Path;

/// Byte-plane width. The payloads are `f32` grids; the axis vectors and the
/// header are a rounding error next to them and ride along harmlessly.
const STRIDE: usize = 4;

/// Container magic — see [`frees_core::props::tables`] for the reader.
const MAGIC: &[u8; 4] = b"FRZ1";

/// Transposes `src` into `STRIDE` byte planes. Any tail shorter than one
/// stride is appended verbatim so the transform stays total.
fn shuffle(src: &[u8]) -> Vec<u8> {
    let n = src.len() / STRIDE;
    let mut out = vec![0u8; src.len()];
    for i in 0..n {
        for b in 0..STRIDE {
            out[b * n + i] = src[i * STRIDE + b];
        }
    }
    out[n * STRIDE..].copy_from_slice(&src[n * STRIDE..]);
    out
}

fn main() {
    let data_dir = Path::new("src/props/data");
    let out_dir = env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let out_dir = Path::new(&out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", data_dir.display());

    // Decision D9: the browser build's property source is rustprop, so it does
    // not link these artifacts (`linked-tables` off) — packing them would be a
    // second of deflate per build for bytes nothing includes. Leaving OUT_DIR
    // empty is also the guard: a `include_bytes!` that escaped its `cfg` fails
    // the build loudly instead of quietly putting 678 KiB back in the bundle.
    if env::var_os("CARGO_FEATURE_LINKED_TABLES").is_none() {
        return;
    }

    let mut entries: Vec<_> = fs::read_dir(data_dir)
        .expect("src/props/data must exist")
        .map(|e| e.expect("readable dir entry").path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("phtab") | Some("fraux")
            )
        })
        .collect();
    // Deterministic order so a rebuild is byte-identical.
    entries.sort();

    for path in entries {
        println!("cargo:rerun-if-changed={}", path.display());
        let raw = fs::read(&path).expect("data file is readable");

        let mut packed = Vec::with_capacity(raw.len());
        packed.extend_from_slice(MAGIC);
        packed.extend_from_slice(
            &u32::try_from(raw.len())
                .expect("artifact under 4 GB")
                .to_le_bytes(),
        );
        packed.extend_from_slice(&miniz_oxide::deflate::compress_to_vec(&shuffle(&raw), 10));

        let name = path.file_name().expect("data file has a name");
        let dest = out_dir.join(format!("{}.frz", name.to_string_lossy()));
        fs::write(&dest, &packed).expect("OUT_DIR is writable");
    }
}
