//! `POST /api/measurements`, `GET /api/measurements/{id}/channels/{name}`,
//! `POST /api/measurements/calc` and `DELETE /api/measurements/{id}`, in the
//! browser.
//!
//! Port of `../frEES/backend/web/.../api/MeasurementController.java` (86 LOC),
//! `MeasurementStore.java` (301) and `MeasurementCalcController.java` (293),
//! over the engine half in [`frees_core::measurement`].
//!
//! # What this boundary deletes
//!
//! Three of the Java store's four jobs exist only because the file was on a
//! server, and all three are gone:
//!
//! * the **upload** — a `.mf4` opened here is read from the user's own disk by
//!   `FileReader` and handed straight to wasm. Nothing is transmitted, which is
//!   the whole point of [`frees_core::measurement`]'s module doc: measurement
//!   recordings are frequently confidential, and the only reason to send one
//!   anywhere was that the parser lived elsewhere;
//! * the **temp directory** with its owner-only permissions, and the TTL sweep
//!   that emptied it. There is no disk, and nothing outlives the tab;
//! * the **`202 Accepted` + `jobId` + poll** path for call-bearing calc
//!   requests. It existed to move a slow evaluation off the API node onto the
//!   compute tier; we are *already* on the worker thread, so
//!   [`measurement_calc`] simply returns the answer.
//!   `measurementApi.ts::calcSignal` still has the polling branch — it is
//!   unreachable through the worker shim, because a wasm call cannot answer
//!   `202`.
//!
//! What survives is the **contract**: the JSON the frontend already parses
//! (`web/src/analyzer/measurementApi.ts` — `RemoteMeasurement`, `RemoteWindow`,
//! `CalcRequestDto`, `CalcResultDto`), so `channelStore.ts` needs no new types.
//!
//! # Failure discipline
//!
//! The same rule as the rest of the boundary: a document problem is data, never
//! a JS exception. Every entry point answers either
//!
//! ```json
//! {"ok": true,  …the Java 200 body…}
//! ```
//!
//! or
//!
//! ```json
//! {"ok": false, "error": {"code": "RASTER_CAP_EXCEEDED", "message": "…",
//!                         "actualPoints": 4000001, "suggestedDt": 0.005, "cap": 1000000}}
//! ```
//!
//! `code` is [`MeasurementError::code`], which is what the frontend switches on;
//! `RASTER_CAP_EXCEEDED` carries the extra fields because the UI offers the
//! suggested `dt` as a one-click fix rather than just refusing.
//!
//! # Non-finite numbers on the wire
//!
//! JSON has no `NaN`, and a gap in measured data *is* `NaN` — never an absent
//! sample (`measurement/mod.rs`). Every `f64` this module emits therefore goes
//! through [`crate::finite_or_null`], the rule the property-plot endpoints
//! already use: a non-finite value is `null`, because silently writing `0`
//! would fabricate a data point.
//!
//! **This puts one requirement on the shim, and it is discharged.**
//! `Float64Array.from([1, null])` is `[1, 0]` — it maps `null` to zero, which
//! is exactly the fabrication the encoding exists to avoid, and
//! `channelStore.ts::remoteWindow` builds its columns that way. The conversion
//! therefore has to happen *upstream* of that call, and it does:
//! `measurementApi.ts::nanFilled` turns every `null` element back into `NaN` as
//! a window or a calc result leaves the API module, so `RemoteWindow.t:
//! number[]` is honest and no consumer ever sees a hole. Verified against both
//! call sites, not assumed — if that helper is ever removed, a dropout in a
//! measured channel silently draws as a **spike to zero** rather than as a gap.
//!
//! The inverse direction is handled here: `JSON.stringify(NaN)` is `null`, so
//! inline calc series arrive with `null` holes and [`nan_filled`] reads them
//! back as `NaN`.
//!
//! **And one open defect, which is not this module's to fix.** `serde_json`'s
//! default float parser is accurate only to within 1 ULP; the exact one is
//! behind its `float_roundtrip` feature, which this workspace does not enable.
//! Numbers this module *emits* are exact (`ryu`), but every number it *reads*
//! can shift by one bit — `1.4000000000000001`, which is what `JSON.stringify`
//! writes for `14 × 0.1`, comes back as `1.4`. Two rules here are exact `f64`
//! equality (`SampledSeries::at`'s exact-hit branch and `raster::union`'s
//! dedupe), so the shift has visible consequences. See
//! `a_json_round_trip_moves_a_timestamp_by_one_ulp` and
//! `a_time_that_arrives_as_json_no_longer_merges_with_the_same_time_from_a_file`
//! below.
//!
//! The remedy is one word in the workspace manifest, and the cost of it is now
//! measured rather than guessed at: building with
//! `serde_json = { version = "1", features = ["float_roundtrip"] }` grows the
//! shipped module by **19.2 KiB** (3 012 809 → 3 032 436 bytes), against
//! **129.8 KiB** of headroom under the 3072 KiB budget. It is affordable. It is
//! left undone here only because the workspace manifest is not this module's
//! file, and because landing it *inverts* the two tests above — they assert the
//! defect is present, deliberately, so that whoever enables the feature is told
//! to rewrite them into assertions that it is gone.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use frees_core::measurement::decimate::{lower_bound, min_max};
use frees_core::measurement::series::{Interp, SampledSeries};
use frees_core::measurement::window::ChannelWindow;
use frees_core::measurement::{
    calc, mdf4, raster, ChannelData, ChannelKind, GroupInfo, MeasurementError, MeasurementMetadata,
    MeasurementSource, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::finite_or_null;

// ---------------------------------------------------------------------------
// Caps
// ---------------------------------------------------------------------------

/// Raster points a call-free formula may span
/// (`MeasurementCalcController.MAX_RASTER`).
const MAX_RASTER: u32 = 1_000_000;

/// Raster points a formula containing a function call may span
/// (`MeasurementCalcController.MAX_RASTER_WITH_CALLS`). One property lookup per
/// sample is orders of magnitude dearer than one add — the Java measured
/// ~107 ns/pt compiled against ~75 µs for an uncached CoolProp call — so a
/// call-bearing formula gets a tenth of the budget.
const MAX_RASTER_WITH_CALLS: u32 = 100_000;

/// Points a channel window returns before it is decimated
/// (`MeasurementController.window`'s `@RequestParam(defaultValue = "2400")`).
const DEFAULT_MAX_POINTS: u32 = 2_400;

/// `Math.min(Math.max(maxPoints, 2), 20_000)`, the same clamp the Java applied
/// to the query parameter. Two is the floor because an envelope bucket needs a
/// min and a max.
const MIN_MAX_POINTS: u32 = 2;
const MAX_MAX_POINTS: u32 = 20_000;

/// Total bytes of opened recordings this module will hold at once.
///
/// **This number has no Java original**, and the one it replaces does not
/// transfer. The Java capped a single *upload* at 200 MB
/// (`frees.security.max-upload-bytes`) and kept 20 files, because its job was
/// to stop one hostile client from filling a shared server's disk. Here the
/// only resource at stake is the user's own tab, so the cap's job changes from
/// policy to survival: the file lives in wasm linear memory, which is 32-bit
/// addressed and shared with the property tables, the component library, the
/// solver and every decoded channel. 512 MiB admits two 200 MB recordings —
/// what a compare workflow needs — while leaving room for a 256 MiB channel
/// read (`mdf4::MAX_RECORDS`) on top of them inside a browser's practical
/// ~2 GiB ceiling.
///
/// A file *larger* than the whole budget is refused rather than evicting
/// everything to make room for something that still will not fit.
const RETAINED_BYTES_BUDGET: u64 = 512 * 1024 * 1024;

/// Opened recordings this module will hold at once, whatever they weigh.
///
/// [`RETAINED_BYTES_BUDGET`] counts a file's **own bytes**, and that is not what
/// an [`Entry`] costs. An [`Mdf4Source`](mdf4::Mdf4Source) also holds `mf4-rs`'s
/// parsed block graph, a per-group record count and this module's copy of the
/// metadata, and those are mostly *fixed* per file rather than proportional to
/// it. So the byte budget is a good bound for big recordings and a bad one for
/// small ones, and it is the small ones that are unbounded in number.
///
/// Measured: the 26 664-byte fixture costs 4 265 bytes of parsed structure
/// beyond its own bytes — 0.16×, immaterial. The *smallest* file this reader
/// accepts (561 bytes: one group, one channel, one record, written by `mf4-rs`'s
/// own writer) costs **3 679 bytes**, or 6.6×. At 512 MiB of *declared* bytes
/// that is 957 000 entries and roughly **4 GB** of real linear memory on wasm32
/// — the entire address space — while `retainedBytes` reports 512 MiB and the
/// eviction loop, correctly by its own lights, never fires.
///
/// Reaching it needs a script calling `measurementApi.ts::uploadMeasurement` in
/// a loop rather than a person picking files, so this is a bound on a foot-gun
/// rather than a defence against an attacker. It is still the same shape as
/// every other defect this surface has had — a bound stated in the wrong unit —
/// and the Java capped the count too (20 files, for its own reasons). 256 is
/// generous against any compare workflow and pins the fixed overhead at about
/// 1 MB; over it, the least-recently-used file is evicted and named, exactly as
/// the byte budget does.
const MAX_OPEN_FILES: usize = 256;

/// Largest decoded channel kept in the one-entry read cache.
///
/// The Java kept 24 decoded channels in an LRU so that zooming did not re-read
/// the file. That policy is unaffordable here — 24 × `MAX_RECORDS` is 6 GiB —
/// but the motivation is unchanged, and pan/zoom re-requests *the same* channel
/// at successive ranges, so one entry captures nearly all of the benefit.
/// Channels above this size are decoded, used and dropped rather than retained.
const CHANNEL_CACHE_BYTES: usize = 32 * 1024 * 1024;

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

/// One opened recording.
///
/// The metadata is read once at [`measurement_open`] and kept, as the Java
/// `MeasurementStore.Entry` kept it: it is header-only, the file cannot change
/// underneath it, and `Mdf4Source::metadata` re-walks every block group per
/// call.
struct Entry {
    name: String,
    /// Bytes of the file as opened — what the wire reports as `size` and what
    /// the budget counts.
    size: u64,
    source: Box<dyn MeasurementSource>,
    metadata: MeasurementMetadata,
    /// Tick of the last access, for LRU eviction.
    touched: u64,
}

/// The most recently decoded channel. See [`CHANNEL_CACHE_BYTES`].
struct CachedChannel {
    id: String,
    group: usize,
    channel: String,
    data: ChannelData,
}

#[derive(Default)]
struct Registry {
    entries: HashMap<String, Entry>,
    /// Both the id source and the LRU clock; see [`Registry::mint_id`].
    counter: u64,
    /// Sum of every held [`Entry::size`].
    retained: u64,
    cache: Option<CachedChannel>,
}

thread_local! {
    /// Opened recordings, keyed by the id [`measurement_open`] minted.
    ///
    /// A `thread_local` rather than a `static Mutex`, which is what the tests
    /// in `lib.rs` use for the property backend — and the difference is worth
    /// stating, because it is not a style choice. The property backend is
    /// genuinely process-global state that `cargo test`'s *parallel* threads
    /// share, so it needs a lock. This registry is the opposite: a dedicated
    /// worker is single-threaded, `engine.worker.ts` instantiates one wasm
    /// module, and nothing else can reach these bytes. A `Mutex` would buy
    /// nothing and would make an opened file cross a thread boundary that does
    /// not exist. (`repl.rs` keeps its solved workspace the same way, for the
    /// same reason.) In tests the harness gives each `#[test]` its own thread,
    /// so each test gets a fresh registry for free.
    ///
    /// **This makes worker affinity a hard requirement.** The file lives in
    /// *this* module instance; a measurement call routed to a different worker
    /// would not find it. `engineClient.ts` keeps exactly one worker, which
    /// satisfies it today — the same constraint the REPL already has.
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    REGISTRY.with(|cell| f(&mut cell.borrow_mut()))
}

impl Registry {
    /// A fresh id, and the tick that orders it against the others.
    ///
    /// One monotonic counter, deliberately: `rand` would drag `getrandom` and
    /// its wasm shim into a bundle that is already over budget, for an
    /// identifier that never leaves the tab and is never a secret; and
    /// `Date.now()` has millisecond resolution, so two files opened in the same
    /// tick would collide and the second would silently answer for the first.
    /// A counter cannot collide within a module instance, and it is readable in
    /// a console log. Ids are only meaningful *inside* one instance — see the
    /// affinity note on [`REGISTRY`]. They are unique but not contiguous: the
    /// same counter is the LRU clock, so an access between two opens shows up
    /// as a skipped number.
    fn mint_id(&mut self) -> String {
        self.counter += 1;
        format!("m{}", self.counter)
    }

    fn tick(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    /// Free room for one incoming file of `incoming` bytes by dropping
    /// least-recently-used entries, returning the ids dropped so the caller can
    /// tell the UI.
    ///
    /// Two budgets, because one number cannot bound both regimes: bytes for big
    /// recordings, and a count for small ones, whose real cost is mostly
    /// per-file rather than per-byte. See [`MAX_OPEN_FILES`].
    fn evict_for(&mut self, incoming: u64) -> Vec<String> {
        let mut evicted = Vec::new();
        while (self.retained + incoming > RETAINED_BYTES_BUDGET
            || self.entries.len() >= MAX_OPEN_FILES)
            && !self.entries.is_empty()
        {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.remove(&oldest);
            evicted.push(oldest);
        }
        evicted
    }

    fn remove(&mut self, id: &str) -> bool {
        let Some(entry) = self.entries.remove(id) else {
            return false;
        };
        self.retained = self.retained.saturating_sub(entry.size);
        // The cache is a copy of samples from a file the caller just asked us
        // to forget. Keeping it would break the promise the whole module is
        // built on.
        if self.cache.as_ref().is_some_and(|c| c.id == id) {
            self.cache = None;
        }
        true
    }

    /// Mark `id` as just used, for the LRU order.
    fn touch(&mut self, id: &str) {
        let tick = self.tick();
        if let Some(entry) = self.entries.get_mut(id) {
            entry.touched = tick;
        }
    }
}

/// Decode a channel (or reuse the one-entry cache) and hand it to `f`.
///
/// Written as a closure rather than as a function returning `&ChannelData`
/// because the cached and freshly-decoded cases have different owners, and a
/// channel too large to cache must not be cloned just to give the two cases one
/// return type — that clone would be up to 256 MiB.
fn with_channel<T>(
    registry: &mut Registry,
    id: &str,
    group: usize,
    channel: &str,
    f: impl FnOnce(&ChannelData) -> T,
) -> Result<T> {
    let hit = registry
        .cache
        .as_ref()
        .is_some_and(|c| c.id == id && c.group == group && c.channel == channel);
    if hit {
        let data = &registry.cache.as_ref().expect("just checked").data;
        return Ok(f(data));
    }

    let Some(entry) = registry.entries.get(id) else {
        return Err(unknown_measurement(registry, id));
    };
    let data = entry.source.channel(group, channel)?;

    // 16 bytes per record: one time and one value.
    if data.len().saturating_mul(16) <= CHANNEL_CACHE_BYTES {
        registry.cache = Some(CachedChannel {
            id: id.to_string(),
            group,
            channel: channel.to_string(),
            data,
        });
        let data = &registry.cache.as_ref().expect("just stored").data;
        Ok(f(data))
    } else {
        Ok(f(&data))
    }
}

// ---------------------------------------------------------------------------
// Errors and JSON helpers
// ---------------------------------------------------------------------------

/// The Java's "Unknown measurement id (the store is ephemeral — re-upload the
/// file)", reworded for a tab: nothing was uploaded, so there is nothing to
/// re-upload.
fn unknown_measurement(registry: &Registry, id: &str) -> MeasurementError {
    MeasurementError::NotFound(format!(
        "Unknown measurement id \"{id}\". Opened files are held in this browser tab only — \
         nothing is uploaded, so re-open the file from disk to get a new id. {}",
        held(registry)
    ))
}

fn unknown_measurement_for_input(registry: &Registry, var: &str, id: &str) -> MeasurementError {
    MeasurementError::NotFound(format!(
        "Input \"{var}\" names measurement id \"{id}\", which this tab does not hold. {}",
        held(registry)
    ))
}

/// Files open right now, so "unknown id" is a signpost rather than a dead end —
/// the same courtesy `mdf4.rs` extends when a channel name misses.
fn held(registry: &Registry) -> String {
    /// Ids listed before the message says "and N more".
    const MAX_LISTED: usize = 20;

    if registry.entries.is_empty() {
        return "No file is open in this tab.".to_string();
    }
    let mut listed: Vec<String> = registry
        .entries
        .iter()
        .map(|(id, entry)| format!("{id} (\"{}\")", entry.name))
        .collect();
    // A HashMap iterates in an arbitrary order; the diagnostic must not.
    listed.sort();
    let total = listed.len();
    listed.truncate(MAX_LISTED);
    let mut joined = listed.join(", ");
    if total > listed.len() {
        joined.push_str(&format!(", and {} more", total - listed.len()));
    }
    format!("Open: {joined}.")
}

/// `{"ok": false, "error": {…}}` — the failure envelope for all four calls.
///
/// [`MeasurementError::RasterCapExceeded`] carries its numbers alongside the
/// message because `CalcSignalModal` offers the suggested `dt` as a button. The
/// suggestion can be non-finite (a `NaN` in a corrupt time master, or a cap of
/// one), and then it arrives as `null`: honest about there being no actionable
/// `dt`, instead of a plausible number that would not work.
fn error_json(error: &MeasurementError) -> String {
    let mut body = json!({
        "code": error.code(),
        "message": error.to_string(),
    });
    if let MeasurementError::RasterCapExceeded {
        actual_points,
        suggested_dt,
        cap,
    } = error
    {
        body["actualPoints"] = json!(actual_points);
        body["suggestedDt"] = finite_or_null(*suggested_dt);
        body["cap"] = json!(cap);
    }
    json!({ "ok": false, "error": body }).to_string()
}

fn reply(result: Result<Value>) -> String {
    match result {
        Ok(value) => value.to_string(),
        Err(error) => error_json(&error),
    }
}

/// A malformed request body. The contract has no "bad request" variant, so this
/// borrows [`MeasurementError::Parse`] and says plainly what failed — see the
/// note in the final report; `Parse`'s doc comment claims it is about MDF4
/// bytes, and it now carries three unrelated things.
fn parse_json<T: for<'de> Deserialize<'de>>(request_json: &str) -> Result<T> {
    serde_json::from_str(request_json)
        .map_err(|e| MeasurementError::Parse(format!("Malformed measurement request: {e}")))
}

/// A `f64` slice as a JSON array, non-finite values as `null`. See the module
/// doc: the shim must read `null` back as `NaN`, not as zero.
fn f64_array(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|v| finite_or_null(*v)).collect())
}

/// `Math.min`: NaN-propagating, `-0.0 < 0.0`. Rust's `f64::min` discards a NaN
/// operand, which would hand the user a plausible-looking `dt` derived from a
/// span that is not actually known. `raster.rs` keeps the same pair private for
/// the same reason.
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max`: NaN-propagating, `0.0 > -0.0`.
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

// ---------------------------------------------------------------------------
// POST /api/measurements  (upload → open)
// ---------------------------------------------------------------------------

/// Parse a `.mf4` recording, keep it, and return its metadata.
///
/// The reply is `RemoteMeasurement` from `measurementApi.ts` — `measurementId`,
/// `name`, `size`, `metadata.groups[]` — plus `evicted` and `retainedBytes`,
/// which the Java had no need for because its store lived on a server with a
/// disk. Extra fields are invisible to the frontend's structural types.
///
/// # Memory
///
/// `bytes` is a `Vec<u8>`, not a `&[u8]`, and that is the single most important
/// line in this file. Both spellings generate *identical* JS glue — one
/// `passArray8ToWasm0` copying the `Uint8Array` into linear memory, and the
/// same `.d.ts` signature — so the difference is invisible from the shim and
/// entirely on the Rust side. That is checked, not assumed: built both ways,
/// the emitted `frees_wasm.js` is byte-for-byte the same and the `&[u8]`
/// module is 133 bytes *larger*, which is the `.to_vec()` and nothing else.
/// `&[u8]` arrives as a `Box<[u8]>` anchor that
/// wasm-bindgen frees when the call returns, which means an owned buffer costs
/// a `.to_vec()`: a second allocation, live at the same time as the anchor.
/// A `Vec<u8>` parameter *is* that allocation, so [`mdf4::open`] can move it
/// into the [`Mdf4Source`](mdf4::Mdf4Source) and the file exists exactly once.
/// On a 200 MB recording the difference is 200 MB of peak linear memory, for
/// nothing.
///
/// The move survives all the way down **on wasm32 only**, and that is a
/// property of `mf4-rs`, not of this signature: its `parse_from_bytes` is
/// `#[cfg]`-split, and the `target_arch = "wasm32"` arm stores the buffer
/// (`mmap: data`) while the native arm copies it into an anonymous mmap. So
/// "exactly once" is true of the target that ships, and a *native* run of these
/// tests peaks at twice the file. Worth knowing before trusting a native
/// memory measurement of an open.
///
/// Whoever hands the bytes over should also transfer the `ArrayBuffer` to the
/// worker rather than copy it — `engine.worker.ts` documents that it does.
///
/// # A second file while the first is open
///
/// Both are kept, up to `RETAINED_BYTES_BUDGET` in total; past that the
/// least-recently-used recordings are dropped and named in `evicted`, so the UI
/// can show the "Locate file…" state it already has for a lost measurement
/// rather than discovering it on the next window read. A file bigger than the
/// whole budget is refused outright — evicting everything for something that
/// still would not fit costs the user their open work twice over.
///
/// Eviction happens *after* the new file parses. A corrupt drop-in therefore
/// cannot cost you the recordings you already had open.
#[wasm_bindgen]
pub fn measurement_open(bytes: Vec<u8>, name: &str) -> String {
    reply(open_inner(bytes, name))
}

/// Refuse a file that could not be held even with the registry empty.
///
/// Separate from [`open_inner`] so it can be tested at a size no test can
/// allocate.
fn refuse_oversize(size: u64) -> Result<()> {
    if size > RETAINED_BYTES_BUDGET {
        return Err(MeasurementError::Parse(format!(
            "This recording is {size} bytes, above the {RETAINED_BYTES_BUDGET}-byte limit a \
             browser tab can hold. Cut it to a shorter time range and re-export it."
        )));
    }
    Ok(())
}

fn open_inner(bytes: Vec<u8>, name: &str) -> Result<Value> {
    let size = bytes.len() as u64;
    refuse_oversize(size)?;

    // Parse before touching the registry: `open` validates the block graph and
    // probes every group's record count, so a file that gets past it cannot
    // fail structurally later.
    let source = mdf4::open(bytes)?;
    let metadata = source.metadata()?;

    Ok(with_registry(|registry| {
        let evicted = registry.evict_for(size);
        let id = registry.mint_id();
        let touched = registry.tick();
        let body = json!({
            "ok": true,
            "measurementId": id,
            "name": name,
            "size": size,
            "metadata": metadata_json(&metadata),
            "evicted": evicted,
            "retainedBytes": registry.retained + size,
        });
        registry.entries.insert(
            id,
            Entry {
                name: name.to_string(),
                size,
                source: Box::new(source),
                metadata,
                touched,
            },
        );
        registry.retained += size;
        body
    }))
}

fn metadata_json(metadata: &MeasurementMetadata) -> Value {
    json!({
        "groups": metadata.groups.iter().map(group_json).collect::<Vec<Value>>(),
    })
}

/// `RemoteGroupInfo` / `RemoteChannelInfo`, camelCase on the wire.
///
/// `kind` here is the **header-derived** one from `mdf4::kind_of` — what the
/// signal browser labels the channel with. The *window* reports a different
/// one; see [`sniff_kind`].
fn group_json(group: &GroupInfo) -> Value {
    json!({
        "index": group.index,
        "name": group.name,
        "records": group.records,
        "channels": group.channels.iter().map(|channel| json!({
            "name": channel.name,
            "unit": channel.unit,
            "timeMaster": channel.time_master,
            "kind": channel.kind.as_str(),
        })).collect::<Vec<Value>>(),
    })
}

// ---------------------------------------------------------------------------
// GET /api/measurements/{id}/channels/{name}
// ---------------------------------------------------------------------------

/// `{measurementId, group, channel, from, to, maxPoints}` — the query the Java
/// took as path variables and query parameters, as one JSON body because a wasm
/// export has no URL. `from`/`to` are `null` for "the whole recording";
/// `maxPoints` defaults to the Java's 2400.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct WindowRequest {
    measurement_id: String,
    group: usize,
    channel: String,
    from: Option<f64>,
    to: Option<f64>,
    max_points: Option<u32>,
}

/// One channel over a time range: raw samples when the range fits `maxPoints`,
/// a min/max envelope otherwise.
///
/// The reply is `RemoteWindow` — `{t, v, min, max, decimated, totalSamples,
/// unit, kind}`, with `v` null on a decimated window and `min`/`max` null on a
/// raw one. `totalSamples` is always the *unreduced* channel length, which is
/// what stops the UI claiming a two-million-sample recording is 800 points
/// long.
#[wasm_bindgen]
pub fn measurement_channel_window(request_json: &str) -> String {
    reply(window_inner(request_json))
}

fn window_inner(request_json: &str) -> Result<Value> {
    let request: WindowRequest = parse_json(request_json)?;
    let max_points = request
        .max_points
        .unwrap_or(DEFAULT_MAX_POINTS)
        .clamp(MIN_MAX_POINTS, MAX_MAX_POINTS) as usize;

    with_registry(|registry| {
        // The unit and the declared kind come from the metadata read at open,
        // so an unreadable channel still reports them — and so this cannot
        // fail for a reason `channel()` would report better a line later.
        let Some(entry) = registry.entries.get(&request.measurement_id) else {
            return Err(unknown_measurement(registry, &request.measurement_id));
        };
        let (unit, declared) = channel_info(&entry.metadata, request.group, &request.channel);
        registry.touch(&request.measurement_id);

        with_channel(
            registry,
            &request.measurement_id,
            request.group,
            &request.channel,
            |data| {
                window_json(&slice_window(
                    data,
                    request.from,
                    request.to,
                    max_points,
                    unit,
                    declared,
                ))
            },
        )
    })
}

/// The declared unit and kind of a channel, by the same name-resolution rule
/// `Mdf4Source::channel` uses: exact spelling first, case-insensitive as a
/// fallback. An unknown group or channel yields defaults; the read that follows
/// is what reports the real error.
fn channel_info(
    metadata: &MeasurementMetadata,
    group_index: usize,
    channel: &str,
) -> (Option<String>, ChannelKind) {
    let Some(group) = metadata.groups.iter().find(|g| g.index == group_index) else {
        return (None, ChannelKind::Analog);
    };
    group
        .channels
        .iter()
        .find(|c| c.name == channel)
        .or_else(|| {
            group
                .channels
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(channel))
        })
        .map_or((None, ChannelKind::Analog), |c| (c.unit.clone(), c.kind))
}

/// `MeasurementStore.getWindow`, statement for statement.
///
/// **Divergence from the Java.** An inverted range (`from > to`, which makes
/// `i1 < i0`) computes a negative count in Java, takes the `count <= maxPoints`
/// branch, and dies in `Arrays.copyOfRange`. Here it returns an empty window:
/// asking for `[5 s, 2 s]` asks for nothing, and under `panic = "abort"` the
/// Java's throw would take the module — and with it the file the user was
/// promised would never leave the tab.
fn slice_window(
    data: &ChannelData,
    from: Option<f64>,
    to: Option<f64>,
    max_points: usize,
    unit: Option<String>,
    declared: ChannelKind,
) -> ChannelWindow {
    // Equal lengths are a `ChannelData` contract, so this `min` never bites for
    // an `Mdf4Source` — which truncates to it. It is here because the raw path
    // slices `v` by an index derived from `t`, and under `panic = "abort"` a
    // source that broke the contract would take the module down rather than
    // report anything. `SampledSeries::at` and `min_max` take the same common
    // prefix for the same reason.
    let n = data.time.len().min(data.values.len());
    let t = &data.time[..n];
    let v = &data.values[..n];
    let kind = sniff_kind(v, declared);
    let total = n as u64;
    if n == 0 {
        return ChannelWindow::raw(Vec::new(), Vec::new(), 0, unit, kind);
    }

    // One sample beyond each edge, so a line entering the view is drawn rather
    // than starting inside it. `channelStore.ts` does the same arithmetic for
    // its in-browser channels — this is the pair that has to agree.
    let i0 = from.map_or(0, |x| lower_bound(t, x)).saturating_sub(1);
    let i1 = to.map_or(n - 1, |x| lower_bound(t, x)).min(n - 1);
    if i1 < i0 {
        return ChannelWindow::raw(Vec::new(), Vec::new(), total, unit, kind);
    }

    let count = i1 - i0 + 1;
    if count <= max_points {
        return ChannelWindow::raw(t[i0..=i1].to_vec(), v[i0..=i1].to_vec(), total, unit, kind);
    }
    // Two points per bucket (a min and a max), so the drawn point count is the
    // budget the caller asked for.
    let buckets = (max_points / 2).max(1);
    ChannelWindow::decimated(min_max(t, v, i0, i1, buckets), total, unit, kind)
}

/// `MeasurementStore.sniffKind`: a channel whose every readable sample is 0 or
/// 1 draws as a boolean.
///
/// Deliberately *not* the header kind, and the Java made the same split: the
/// signal browser labels a channel from its header, while the chart asks what
/// the samples actually look like. It matters because `cn_val_range` is
/// optional and asammdf leaves it clear, so a genuine `uint8` 0/1 channel comes
/// out of `mdf4::kind_of` as `Analog` — the sniff is what makes it plot
/// stepped. It runs over the whole channel, not the visible slice, so the
/// rendering does not change as you zoom.
///
/// **One divergence.** A text channel's samples are all `NaN`, so the Java's
/// sniff answered `"analog"` for it and invited plotting a channel with nothing
/// to plot. The header knows better, so a declared text channel stays text.
fn sniff_kind(values: &[f64], declared: ChannelKind) -> ChannelKind {
    if declared == ChannelKind::Text {
        return ChannelKind::Text;
    }
    let mut saw_value = false;
    for &x in values {
        if x.is_nan() {
            continue;
        }
        saw_value = true;
        if x != 0.0 && x != 1.0 {
            return ChannelKind::Analog;
        }
    }
    if saw_value {
        ChannelKind::Boolean
    } else {
        ChannelKind::Analog
    }
}

fn window_json(window: &ChannelWindow) -> Value {
    json!({
        "ok": true,
        "t": f64_array(&window.t),
        "v": window.v.as_deref().map(f64_array),
        "min": window.min.as_deref().map(f64_array),
        "max": window.max.as_deref().map(f64_array),
        "decimated": window.decimated,
        "totalSamples": window.total_samples,
        "unit": window.unit,
        "kind": window.kind.as_str(),
    })
}

// ---------------------------------------------------------------------------
// POST /api/measurements/calc
// ---------------------------------------------------------------------------

/// `CalcRequestDto` in `measurementApi.ts` (`MeasurementCalcController.CalcRequest`).
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CalcRequest {
    name: String,
    formula: String,
    inputs: Vec<CalcInput>,
    raster: Option<RasterSpec>,
}

/// One bound input: either an inline series or a channel of an open file.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CalcInput {
    var: String,
    interp: Option<String>,
    measurement_id: Option<String>,
    channel: Option<String>,
    group: Option<usize>,
    inline: Option<InlineSeries>,
}

/// `t`/`v` are `Option<f64>` because `JSON.stringify(NaN)` is `null`: a browser
/// -built series with a gap in it *will* arrive with holes, and refusing it
/// would refuse every imported CSV channel that has one. See [`nan_filled`].
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct InlineSeries {
    t: Vec<Option<f64>>,
    v: Vec<Option<f64>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RasterSpec {
    mode: Option<String>,
    dt: Option<f64>,
    same_as: Option<String>,
}

/// Evaluate one frees formula at every point of a raster built from its inputs.
///
/// The reply is `CalcResultDto` — `{name, t, v}`. A raster that would exceed
/// the point cap comes back as `RASTER_CAP_EXCEEDED` carrying a `dt` that
/// verifiably fits, which is the difference between a guided path and a
/// refusal.
///
/// Always synchronous: the Java sent call-bearing formulas over 10 000 points
/// to the compute tier as a job, and there is no compute tier here — this call
/// already runs on the worker thread, off the UI. A million-point property
/// formula will take as long as it takes, and the tab stays responsive.
#[wasm_bindgen]
pub fn measurement_calc(request_json: &str) -> String {
    reply(calc_inner(request_json))
}

fn calc_inner(request_json: &str) -> Result<Value> {
    // A formula may call `enthalpy(…)`. `start()` installs the property tables
    // at module init, so in the browser this is already done; the call keeps
    // the function correct when it is reached without one (native tests, and
    // any future host that skips the start hook).
    frees_core::props::tables::install_builtin_once();

    let request: CalcRequest = parse_json(request_json)?;
    if request.formula.trim().is_empty() {
        return Err(MeasurementError::Parse("The formula is empty.".to_string()));
    }
    let formula = calc::parse_formula(&request.formula)?;
    let cap = if calc::contains_call(&formula) {
        MAX_RASTER_WITH_CALLS
    } else {
        MAX_RASTER
    };

    let inputs = with_registry(|registry| resolve_inputs(registry, &request.inputs))?;
    if inputs.is_empty() {
        return Err(MeasurementError::Parse(
            "Bind at least one input signal.".to_string(),
        ));
    }

    let raster = build_raster(request.raster.as_ref(), &inputs, cap)?;
    let values = calc::evaluate(&formula, &raster, &inputs)?;
    Ok(json!({
        "ok": true,
        "name": request.name,
        "t": f64_array(&raster),
        "v": f64_array(&values),
    }))
}

/// Resolve every input to a [`SampledSeries`], keyed by the variable name the
/// formula uses.
///
/// Keyed by the caller's **own spelling**, not lowercased as the Java did.
/// `calc::evaluate` lowercases the keys itself and refuses two inputs that
/// differ only in case — frees names are case-insensitive, so `Speed` and
/// `speed` are one variable and binding both is genuinely ambiguous. Lowercasing
/// here would silently drop one of them, which is what the Java's
/// `LinkedHashMap.put` did.
fn resolve_inputs(
    registry: &mut Registry,
    inputs: &[CalcInput],
) -> Result<BTreeMap<String, SampledSeries>> {
    if inputs.len() > MAX_INPUTS {
        return Err(MeasurementError::Parse(format!(
            "This request binds {} input signals, above the {MAX_INPUTS} a calculated signal may \
             use. A formula can only name a handful; bind the ones it reads.",
            inputs.len()
        )));
    }
    let mut out: BTreeMap<String, SampledSeries> = BTreeMap::new();
    let mut budget = SampleBudget::default();
    for input in inputs {
        // Trimmed, unlike the Java: `" speed"` there became a binding no
        // formula could ever name, and the user got "unknown variable" for a
        // signal they had visibly bound.
        let var = input.var.trim();
        if var.is_empty() {
            return Err(MeasurementError::Parse(
                "An input is missing its variable name.".to_string(),
            ));
        }
        let interp = parse_interp(input.interp.as_deref(), var)?;

        let series = match &input.inline {
            Some(inline) => {
                if inline.t.len() != inline.v.len() {
                    return Err(MeasurementError::Parse(format!(
                        "Inline series for \"{var}\" is malformed: {} time(s) against {} value(s).",
                        inline.t.len(),
                        inline.v.len()
                    )));
                }
                budget.claim(var, inline.t.len() as u64)?;
                SampledSeries::new(nan_filled(&inline.t), nan_filled(&inline.v), interp)
            }
            None => {
                let Some(channel) = input.channel.as_deref() else {
                    return Err(MeasurementError::Parse(format!(
                        "Input \"{var}\" binds neither an inline series nor a channel of an open \
                         measurement."
                    )));
                };
                let id = input.measurement_id.as_deref().unwrap_or_default();
                let group = input.group.unwrap_or(0);
                // Claimed from the metadata *before* the read, because the
                // whole point is not to materialise it. `records` is
                // header-derived, so this costs nothing.
                budget.claim(var, declared_records(registry, id, group))?;
                let data = read_channel(registry, var, id, group, channel)?;
                // Moved, not copied: a full-resolution channel is the largest
                // thing this module handles.
                SampledSeries::new(data.time, data.values, interp)
            }
        };

        if out.insert(var.to_string(), series).is_some() {
            return Err(MeasurementError::Formula(format!(
                "Input \"{var}\" is bound twice. Bind each formula variable once."
            )));
        }
    }
    Ok(out)
}

/// Samples every bound input may materialise between them, before the raster
/// cap has anything to say about it.
///
/// The two caps this sits between are both about the *answer*: `MAX_RASTER`
/// bounds the output, and `mdf4::MAX_RECORDS` bounds one channel read (the
/// same 16 777 216, which is 256 MiB of paired times and values). Nothing
/// bounded the **sum**, and the calc path reads every input in full before the
/// raster is built — so ten variables naming the same 16 M-sample channel is a
/// 2.7 GB request from a request body of a few hundred bytes, answered on
/// wasm32 by an allocator trap rather than by a diagnostic. This is the same
/// defect shape the Phase 7–8 sweep closed with `ode::problem::MAX_OUTPUT_SAMPLES`.
///
/// One channel read's worth is the right ceiling because it is the number the
/// reader already promises to survive, and it leaves the case the merge
/// *exists* for untouched: ten channels on one 100 kHz master for ten seconds
/// is ten million samples merging to one raster, and it fits.
const MAX_INPUT_SAMPLES: u64 = 16_777_216;

/// Input signals one calculated signal may bind.
///
/// `calc::evaluate` refreshes **every** column at **every** raster point — the
/// slot buffer for all of them, and, inside a `Call`, the scratch scope for all
/// of them too — so its cost is `raster × inputs`, and the request sets both
/// numbers independently and cheaply. One-point inline series make the raster
/// as long as the input list, which turns the whole call quadratic in the size
/// of the request body. Measured in release: 2 000 such inputs took 0.08 s,
/// 8 000 took 2.2 s, and **32 000 took 52 s** — from about 1.8 MB of JSON, with
/// the worker wedged and nothing able to cancel it.
///
/// [`MAX_INPUT_SAMPLES`] does not close that: a million one-point inputs is a
/// million samples, well inside it. The count is the number that has to be
/// bounded, and it is a comfortable bound to set — a formula names a handful of
/// variables, and the parser's depth budget stops it naming very many more.
/// Measured at this ceiling with a full million-point raster, the whole
/// evaluation is 2.2 s in release; at 512 it is 9.8 s.
///
/// **This bounds the time, not the memory, and the two do not follow each
/// other.** `evaluate` holds one raster-length column per input at once, so 128
/// inputs on a million-point raster is a gigabyte — measured at 1 044 MB through
/// this very entry point, from a 5 604-byte request body. That product is
/// bounded by `frees_core::measurement::calc`'s own `MAX_INPUT_COLUMN_SAMPLES`, which
/// is where the allocation happens; this constant is not the guard for it and
/// raising it would not move that ceiling.
const MAX_INPUTS: usize = 128;

/// Running total of the samples the bound inputs will hold at once.
#[derive(Default)]
struct SampleBudget {
    used: u64,
}

impl SampleBudget {
    fn claim(&mut self, var: &str, samples: u64) -> Result<()> {
        self.used = self.used.saturating_add(samples);
        if self.used > MAX_INPUT_SAMPLES {
            return Err(MeasurementError::Parse(format!(
                "The bound inputs come to at least {} samples once \"{var}\" is included, above \
                 the {MAX_INPUT_SAMPLES}-sample limit a browser tab can hold at once. Bind fewer \
                 signals, or narrow the recording and re-export it.",
                self.used
            )));
        }
        Ok(())
    }
}

/// How many samples a channel of `group` will decode to, from the metadata read
/// at open — header-derived, so asking costs nothing.
///
/// An unknown id or group answers zero rather than erroring: the read that
/// follows reports those, and with a better message than a budget check could.
fn declared_records(registry: &Registry, id: &str, group: usize) -> u64 {
    registry
        .entries
        .get(id)
        .and_then(|entry| entry.metadata.groups.iter().find(|g| g.index == group))
        .map_or(0, |g| g.records)
}

/// A full-resolution channel for the calc engine.
///
/// Bypasses the window cache in both directions: a calc reads each input
/// exactly once, so caching its result would evict the channel the chart is
/// panning over to store something nothing will ask for again.
fn read_channel(
    registry: &mut Registry,
    var: &str,
    id: &str,
    group: usize,
    channel: &str,
) -> Result<ChannelData> {
    registry.touch(id);
    let Some(entry) = registry.entries.get(id) else {
        return Err(unknown_measurement_for_input(registry, var, id));
    };
    entry.source.channel(group, channel)
}

/// The wire spelling `measurementApi.ts` declares (`'step' | 'linear'`), read
/// case-insensitively as the Java's `equalsIgnoreCase` did.
///
/// **Divergence, deliberate.** The Java's `else` branch made *any* unrecognised
/// string mean `LINEAR`. That is the wrong default to guess at: `step` is what
/// `CalcSignalModal` picks for a boolean channel, and linearly interpolating a
/// valve position across a gap invents intermediate openings that never
/// happened. A typo therefore fails here with the modes named. An absent or
/// empty field still means `linear` — that is a caller declining to choose, not
/// a caller choosing wrongly.
fn parse_interp(raw: Option<&str>, var: &str) -> Result<Interp> {
    match raw.map(str::trim) {
        None | Some("") => Ok(Interp::Linear),
        Some(mode) if mode.eq_ignore_ascii_case(Interp::Step.as_str()) => Ok(Interp::Step),
        Some(mode) if mode.eq_ignore_ascii_case(Interp::Linear.as_str()) => Ok(Interp::Linear),
        Some(other) => Err(MeasurementError::Parse(format!(
            "Input \"{var}\" asks for interpolation \"{other}\", which is not a mode this engine \
             has. Use \"step\" or \"linear\"."
        ))),
    }
}

/// JSON `null` back to `NaN` — the inverse of [`f64_array`], and the reason
/// inline series deserialise as `Option<f64>`.
fn nan_filled(values: &[Option<f64>]) -> Vec<f64> {
    values.iter().map(|v| v.unwrap_or(f64::NAN)).collect()
}

/// `MeasurementCalcController.buildRaster`.
///
/// `sameAs` copies the named input's own time base where the Java aliased it.
/// The copy is bounded by `cap` (checked first), so it is at most 8 MB, and
/// aliasing it would mean handing out a second owner of a series the evaluator
/// is about to read.
fn build_raster(
    spec: Option<&RasterSpec>,
    inputs: &BTreeMap<String, SampledSeries>,
    cap: u32,
) -> Result<Vec<f64>> {
    let mode = spec.and_then(|s| s.mode.as_deref()).unwrap_or("merge");
    let (t0, t1) = span_of(inputs);
    match mode {
        "merge" => {
            let bases: Vec<&[f64]> = inputs.values().map(|s| s.t.as_slice()).collect();
            raster::union(&bases, cap)
        }
        "fixed" => match spec.and_then(|s| s.dt) {
            // NaN fails this guard, which is the Java's `!(spec.dt() > 0)`.
            Some(dt) if dt > 0.0 => raster::fixed(t0, t1, dt, cap),
            _ => Err(MeasurementError::Parse(
                "The fixed raster needs dt > 0.".to_string(),
            )),
        },
        "sameAs" => {
            let name = spec.and_then(|s| s.same_as.as_deref()).unwrap_or("");
            let base = inputs
                .iter()
                .find(|(var, _)| var.eq_ignore_ascii_case(name))
                .map(|(_, series)| series)
                .ok_or_else(|| {
                    MeasurementError::Parse(format!(
                        "sameAs raster: \"{name}\" is not one of the bound inputs."
                    ))
                })?;
            if base.t.len() as u64 > u64::from(cap) {
                return Err(MeasurementError::RasterCapExceeded {
                    actual_points: base.t.len() as u64,
                    suggested_dt: raster::suggest_dt(t0, t1, cap),
                    cap,
                });
            }
            Ok(base.t.clone())
        }
        other => Err(MeasurementError::Parse(format!(
            "Unknown raster mode: {other}"
        ))),
    }
}

/// The inputs' combined extent, from each series' first and last sample — the
/// Java's own approximation, kept: it is only ever used to size a *suggested*
/// `dt`, and a full scan of ten million samples to refine a suggestion nobody
/// has accepted yet is not worth it.
fn span_of(inputs: &BTreeMap<String, SampledSeries>) -> (f64, f64) {
    let mut t0 = f64::INFINITY;
    let mut t1 = f64::NEG_INFINITY;
    for series in inputs.values() {
        if let (Some(&first), Some(&last)) = (series.t.first(), series.t.last()) {
            t0 = java_min(t0, first);
            t1 = java_max(t1, last);
        }
    }
    (t0, t1)
}

// ---------------------------------------------------------------------------
// DELETE /api/measurements/{id}
// ---------------------------------------------------------------------------

/// Forget an opened recording and free its bytes.
///
/// Fire-and-forget, like the Java endpoint and like `repl_clear`: an unknown id
/// is a no-op, because `measurementApi.ts::deleteMeasurement` discards the
/// result and there is no one left to tell.
///
/// This is the *only* way a file is released early — there is no TTL sweep,
/// because a browser tab has no scheduler and the file dies with the tab
/// anyway. The one automatic release is `RETAINED_BYTES_BUDGET` eviction on
/// the next open.
#[wasm_bindgen]
pub fn measurement_close(measurement_id: &str) {
    with_registry(|registry| registry.remove(measurement_id));
}

// ---------------------------------------------------------------------------
// Tests — wire shapes, the slicing arithmetic, and the failure envelope.
//
// The registry is thread-local and the harness gives each `#[test]` its own
// thread, so these do not interfere even though they all write to it.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use frees_core::measurement::ChannelInfo;

    /// A `MeasurementSource` made of arrays.
    ///
    /// The window and calc paths are boundary logic — slicing, decimation,
    /// JSON — and this exercises them without an MDF4 file. There is no `.mf4`
    /// fixture in this repo, and `Mdf4Source` itself is already proven against
    /// a real recording in `frees-core`'s own tests.
    struct FakeSource {
        metadata: MeasurementMetadata,
        channels: BTreeMap<(usize, String), ChannelData>,
    }

    impl MeasurementSource for FakeSource {
        fn metadata(&self) -> Result<MeasurementMetadata> {
            Ok(self.metadata.clone())
        }

        fn channel(&self, group_index: usize, channel_name: &str) -> Result<ChannelData> {
            self.channels
                .get(&(group_index, channel_name.to_string()))
                .cloned()
                .ok_or_else(|| {
                    MeasurementError::NotFound(format!(
                        "Channel \"{channel_name}\" is not in channel group {group_index}."
                    ))
                })
        }
    }

    /// Register a source directly, as `measurement_open` would. `size` is
    /// claimed rather than measured, which is what lets the eviction test hold
    /// "400 MiB" without allocating it.
    fn register(name: &str, size: u64, source: FakeSource) -> String {
        let metadata = source.metadata().expect("fake metadata");
        with_registry(|registry| {
            registry.evict_for(size);
            let id = registry.mint_id();
            let touched = registry.tick();
            registry.entries.insert(
                id.clone(),
                Entry {
                    name: name.to_string(),
                    size,
                    source: Box::new(source),
                    metadata,
                    touched,
                },
            );
            registry.retained += size;
            id
        })
    }

    fn ramp(n: usize) -> FakeSource {
        let time: Vec<f64> = (0..n).map(|i| i as f64 * 0.1).collect();
        let values: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let flags: Vec<f64> = (0..n).map(|i| f64::from(u8::from(i % 10 == 0))).collect();
        let mut channels = BTreeMap::new();
        channels.insert(
            (0, "time".to_string()),
            ChannelData {
                time: time.clone(),
                values: time.clone(),
            },
        );
        channels.insert(
            (0, "speed".to_string()),
            ChannelData {
                time: time.clone(),
                values,
            },
        );
        channels.insert(
            (0, "valve".to_string()),
            ChannelData {
                time,
                values: flags,
            },
        );
        FakeSource {
            metadata: MeasurementMetadata {
                groups: vec![GroupInfo {
                    index: 0,
                    name: "group 0".to_string(),
                    records: n as u64,
                    channels: vec![
                        ChannelInfo {
                            name: "time".to_string(),
                            unit: Some("s".to_string()),
                            time_master: true,
                            kind: ChannelKind::Analog,
                        },
                        ChannelInfo {
                            name: "speed".to_string(),
                            unit: Some("m/s".to_string()),
                            time_master: false,
                            kind: ChannelKind::Analog,
                        },
                        ChannelInfo {
                            name: "valve".to_string(),
                            unit: None,
                            time_master: false,
                            kind: ChannelKind::Analog,
                        },
                    ],
                }],
            },
            channels,
        }
    }

    fn parsed(payload: &str) -> Value {
        serde_json::from_str(payload).expect("boundary output must be valid JSON")
    }

    fn window(id: &str, channel: &str, from: Option<f64>, to: Option<f64>, max: u32) -> Value {
        let request = json!({
            "measurementId": id,
            "group": 0,
            "channel": channel,
            "from": from,
            "to": to,
            "maxPoints": max,
        });
        parsed(&measurement_channel_window(&request.to_string()))
    }

    fn numbers(value: &Value) -> Vec<f64> {
        value
            .as_array()
            .expect("array")
            .iter()
            .map(|v| v.as_f64().unwrap_or(f64::NAN))
            .collect()
    }

    // ── the failure envelope ────────────────────────────────────────────────

    #[test]
    fn a_bad_file_is_data_not_a_trap() {
        let payload = parsed(&measurement_open(b"hello, world".to_vec(), "notes.txt"));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "MEASUREMENT_PARSE_FAILED");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message")
                .contains("MDF4"),
            "{payload}"
        );
    }

    #[test]
    fn an_unknown_id_is_reported_not_guessed_at() {
        let payload = window("m404", "speed", None, None, 100);
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "CHANNEL_NOT_FOUND");
        assert!(
            payload["error"]["message"]
                .as_str()
                .expect("message")
                .contains("m404"),
            "the diagnostic must quote what the caller asked for: {payload}"
        );
    }

    #[test]
    fn a_malformed_request_body_never_throws() {
        for payload in [
            measurement_channel_window("{not json"),
            measurement_calc("{not json"),
        ] {
            let value = parsed(&payload);
            assert_eq!(value["ok"], json!(false), "{value}");
            assert!(value["error"]["message"].is_string(), "{value}");
        }
    }

    // ── RemoteMeasurement ───────────────────────────────────────────────────

    #[test]
    fn the_open_reply_is_the_remote_measurement_shape() {
        let id = register("drive.mf4", 26_664, ramp(1000));
        // `register` is `open_inner` minus the parse, so assert the metadata
        // rendering the reply would have carried.
        let metadata =
            with_registry(|registry| metadata_json(&registry.entries[&id].metadata.clone()));
        let group = &metadata["groups"][0];
        assert_eq!(group["index"], json!(0));
        assert_eq!(group["name"], "group 0");
        assert_eq!(group["records"], json!(1000));
        let channel = &group["channels"][1];
        assert_eq!(channel["name"], "speed");
        assert_eq!(channel["unit"], "m/s");
        assert_eq!(channel["timeMaster"], json!(false));
        assert_eq!(channel["kind"], "analog");
        // A channel with no recorded unit is null, not "" — `unit?: string | null`.
        assert_eq!(group["channels"][2]["unit"], Value::Null);
    }

    // ── RemoteWindow ────────────────────────────────────────────────────────

    #[test]
    fn a_small_range_comes_back_raw() {
        let id = register("drive.mf4", 1, ramp(100));
        let payload = window(&id, "speed", None, None, 2400);
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["decimated"], json!(false));
        assert_eq!(payload["totalSamples"], json!(100));
        assert_eq!(payload["unit"], "m/s");
        assert_eq!(payload["kind"], "analog");
        assert_eq!(payload["t"].as_array().expect("t").len(), 100);
        assert_eq!(payload["v"].as_array().expect("v").len(), 100);
        assert_eq!(payload["min"], Value::Null);
        assert_eq!(payload["max"], Value::Null);
    }

    /// The envelope replaces `v`, and `totalSamples` keeps reporting the
    /// unreduced length — the field that stops the UI claiming the recording is
    /// as short as the picture of it.
    #[test]
    fn a_large_range_comes_back_as_an_envelope() {
        let id = register("drive.mf4", 1, ramp(10_000));
        let payload = window(&id, "speed", None, None, 100);
        assert_eq!(payload["decimated"], json!(true), "{payload}");
        assert_eq!(payload["v"], Value::Null);
        assert_eq!(payload["totalSamples"], json!(10_000));
        assert_eq!(payload["t"].as_array().expect("t").len(), 50);
        assert_eq!(payload["min"].as_array().expect("min").len(), 50);
        assert_eq!(payload["max"].as_array().expect("max").len(), 50);
        // Bucket 0 spans samples 0..199 of a 0,1,2,… ramp.
        assert_eq!(numbers(&payload["min"])[0], 0.0);
        assert_eq!(numbers(&payload["max"])[0], 199.0);
    }

    /// `maxPoints` is a *point* budget, and an envelope draws two per bucket.
    #[test]
    fn the_bucket_count_is_half_the_point_budget() {
        let id = register("drive.mf4", 1, ramp(10_000));
        for max_points in [2u32, 100, 2400] {
            let payload = window(&id, "speed", None, None, max_points);
            assert_eq!(
                payload["t"].as_array().expect("t").len(),
                (max_points / 2) as usize,
                "{payload}"
            );
        }
    }

    /// The Java's `i0 - 1`: one sample before the window, so the line entering
    /// the view is drawn instead of starting inside it.
    #[test]
    fn the_range_reaches_one_sample_beyond_each_edge() {
        let id = register("drive.mf4", 1, ramp(100));
        // Samples are at 0.0, 0.1, …; ask for [1.0, 2.0] — samples 10..20.
        let payload = window(&id, "speed", Some(1.0), Some(2.0), 2400);
        let v = numbers(&payload["v"]);
        assert_eq!(v.first().copied(), Some(9.0), "{payload}");
        assert_eq!(v.last().copied(), Some(20.0), "{payload}");
    }

    /// In Java this is a negative count into `Arrays.copyOfRange`, i.e. a
    /// throw; under `panic = "abort"` that would take the module down.
    #[test]
    fn an_inverted_range_returns_nothing_rather_than_aborting() {
        let id = register("drive.mf4", 1, ramp(100));
        let payload = window(&id, "speed", Some(5.0), Some(2.0), 2400);
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["t"], json!([]));
        assert_eq!(payload["v"], json!([]));
        assert_eq!(payload["totalSamples"], json!(100));
    }

    #[test]
    fn an_empty_channel_is_an_empty_window() {
        let mut source = ramp(0);
        source.channels.insert(
            (0, "speed".to_string()),
            ChannelData {
                time: Vec::new(),
                values: Vec::new(),
            },
        );
        let id = register("empty.mf4", 1, source);
        let payload = window(&id, "speed", None, None, 2400);
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["totalSamples"], json!(0));
        assert_eq!(payload["t"], json!([]));
    }

    /// A source that broke `ChannelData`'s equal-length contract would index
    /// `v` past its end on the raw path — an abort, not an error, in wasm.
    #[test]
    fn a_ragged_channel_truncates_instead_of_aborting() {
        let mut source = ramp(4);
        source.channels.insert(
            (0, "speed".to_string()),
            ChannelData {
                time: vec![0.0, 0.1, 0.2, 0.3],
                values: vec![1.0, 2.0],
            },
        );
        let id = register("ragged.mf4", 1, source);
        let payload = window(&id, "speed", None, None, 2400);
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["t"], json!([0.0, 0.1]));
        assert_eq!(payload["v"], json!([1.0, 2.0]));
        assert_eq!(payload["totalSamples"], json!(2));
    }

    /// A 0/1 channel that the header calls analog still draws stepped, because
    /// the window sniffs the samples — `MeasurementStore.sniffKind`.
    #[test]
    fn a_zero_one_channel_is_sniffed_as_boolean() {
        let id = register("drive.mf4", 1, ramp(100));
        assert_eq!(window(&id, "valve", None, None, 2400)["kind"], "boolean");
        assert_eq!(window(&id, "speed", None, None, 2400)["kind"], "analog");
    }

    #[test]
    fn the_sniff_matches_the_java_case_for_case() {
        // All-NaN: the Java's `sawFinite` stays false → analog.
        assert_eq!(
            sniff_kind(&[f64::NAN, f64::NAN], ChannelKind::Analog),
            ChannelKind::Analog
        );
        // A gap between zeros and ones is skipped, not disqualifying.
        assert_eq!(
            sniff_kind(&[0.0, f64::NAN, 1.0], ChannelKind::Analog),
            ChannelKind::Boolean
        );
        // An infinity is a value, and it is not 0 or 1.
        assert_eq!(
            sniff_kind(&[0.0, f64::INFINITY], ChannelKind::Analog),
            ChannelKind::Analog
        );
        // A declared text channel stays text where the Java said "analog".
        assert_eq!(
            sniff_kind(&[f64::NAN], ChannelKind::Text),
            ChannelKind::Text
        );
    }

    /// A gap must not arrive as a fabricated zero. JSON has no NaN, so it is
    /// `null` — see the module doc for what the shim owes in return.
    #[test]
    fn a_gap_crosses_the_wire_as_null() {
        let mut source = ramp(4);
        source.channels.insert(
            (0, "speed".to_string()),
            ChannelData {
                time: vec![0.0, 0.1, 0.2, 0.3],
                values: vec![1.0, f64::NAN, f64::INFINITY, 4.0],
            },
        );
        let id = register("gaps.mf4", 1, source);
        let payload = window(&id, "speed", None, None, 2400);
        assert_eq!(payload["v"], json!([1.0, Value::Null, Value::Null, 4.0]));
    }

    #[test]
    fn max_points_is_clamped_to_the_java_range() {
        let id = register("drive.mf4", 1, ramp(10_000));
        // 0 clamps up to 2 → one bucket over the whole channel. Its time is the
        // *midpoint sample*, `min_max`'s representative point — a time that was
        // actually measured, not a computed bucket centre.
        let narrow = window(&id, "speed", None, None, 0);
        assert_eq!(narrow["decimated"], json!(true), "{narrow}");
        assert_eq!(narrow["t"].as_array().expect("t").len(), 1);
        assert_eq!(numbers(&narrow["min"]), vec![0.0]);
        assert_eq!(numbers(&narrow["max"]), vec![9999.0]);
        // 10^6 clamps down to 20 000, which is more than the channel has → raw.
        let wide = window(&id, "speed", None, None, 1_000_000);
        assert_eq!(wide["decimated"], json!(false), "{wide}");
        assert_eq!(wide["t"].as_array().expect("t").len(), 10_000);
    }

    // ── the registry ────────────────────────────────────────────────────────

    #[test]
    fn ids_are_unique_and_close_forgets_exactly_one() {
        let first = register("a.mf4", 10, ramp(10));
        let second = register("b.mf4", 10, ramp(10));
        assert_ne!(first, second);
        measurement_close(&first);
        assert_eq!(window(&first, "speed", None, None, 100)["ok"], json!(false));
        assert_eq!(window(&second, "speed", None, None, 100)["ok"], json!(true));
        // An unknown id is a no-op, not a failure.
        measurement_close("m404");
    }

    #[test]
    fn closing_releases_the_budget_and_the_cache() {
        let id = register("a.mf4", 4096, ramp(10));
        let _ = window(&id, "speed", None, None, 100);
        with_registry(|registry| {
            assert_eq!(registry.retained, 4096);
            assert!(registry.cache.is_some(), "the read should have cached");
        });
        measurement_close(&id);
        with_registry(|registry| {
            assert_eq!(registry.retained, 0);
            assert!(
                registry.cache.is_none(),
                "a closed file must not leave its samples behind"
            );
        });
    }

    /// Two 400 MiB recordings do not fit in the 512 MiB budget, so opening the
    /// second drops the first — and says so, rather than letting the UI find
    /// out on its next read.
    #[test]
    fn a_second_large_file_evicts_the_least_recently_used_one() {
        let big = 400 * 1024 * 1024;
        let first = register("a.mf4", big, ramp(10));
        let evicted = with_registry(|registry| registry.evict_for(big));
        assert_eq!(evicted, vec![first.clone()]);
        with_registry(|registry| {
            assert_eq!(registry.retained, 0);
            assert!(!registry.entries.contains_key(&first));
        });
    }

    /// Small files never trip the byte budget, and their real cost is mostly
    /// per-file rather than per-byte — so without a count cap the registry grows
    /// without bound. Measured: the smallest file this reader accepts is 561
    /// bytes and costs 3 679 bytes of parsed structure to hold *beyond* its own
    /// bytes (6.6×), so 512 MiB of *declared* bytes is 957 000 entries and about
    /// 4 GB of real wasm32 memory, with `retainedBytes` reporting 512 MiB and
    /// the eviction loop, correctly by its own lights, never firing. Two
    /// budgets, and the count is the one small files bite.
    #[test]
    fn holding_many_tiny_files_evicts_on_the_count_as_well_as_the_bytes() {
        // A byte apiece: 300 of these is 300 bytes against a 512 MiB budget, so
        // nothing but the count cap can stop them.
        let mut ids = Vec::new();
        for i in 0..MAX_OPEN_FILES + 44 {
            ids.push(register(&format!("f{i}.mf4"), 1, ramp(2)));
        }
        with_registry(|registry| {
            assert_eq!(
                registry.entries.len(),
                MAX_OPEN_FILES,
                "the registry must not grow past its file count"
            );
            // LRU, so it is the *oldest* that went and the newest that stayed.
            assert!(!registry.entries.contains_key(&ids[0]));
            assert!(registry.entries.contains_key(ids.last().expect("opened")));
            // And the byte accounting stayed consistent with what is held.
            assert_eq!(registry.retained, MAX_OPEN_FILES as u64);
        });
    }

    /// Driven through the guard rather than through `measurement_open`: no test
    /// should allocate half a gigabyte to prove an inequality.
    #[test]
    fn a_file_larger_than_the_whole_budget_is_refused() {
        assert!(refuse_oversize(RETAINED_BYTES_BUDGET).is_ok());
        let refusal = refuse_oversize(RETAINED_BYTES_BUDGET + 1).expect_err("must refuse");
        assert_eq!(refusal.code(), "MEASUREMENT_PARSE_FAILED");
        // The user's remedy has to be in the message; the number alone is not one.
        assert!(refusal.to_string().contains("re-export"), "{refusal}");
    }

    #[test]
    fn the_read_cache_serves_a_second_window_of_the_same_channel() {
        let id = register("drive.mf4", 1, ramp(100));
        let first = window(&id, "speed", None, Some(1.0), 2400);
        let second = window(&id, "speed", Some(1.0), None, 2400);
        assert_eq!(first["ok"], json!(true));
        assert_eq!(second["ok"], json!(true));
        with_registry(|registry| {
            let cache = registry.cache.as_ref().expect("cached");
            assert_eq!(cache.channel, "speed");
            assert_eq!(cache.id, id);
        });
    }

    // ── CalcResultDto ───────────────────────────────────────────────────────

    fn calc(request: Value) -> Value {
        parsed(&measurement_calc(&request.to_string()))
    }

    fn inline(var: &str, t: Vec<f64>, v: Vec<f64>, interp: &str) -> Value {
        json!({"var": var, "interp": interp, "inline": {"t": t, "v": v}})
    }

    #[test]
    fn a_merged_raster_evaluates_the_formula_at_every_union_point() {
        let payload = calc(json!({
            "name": "power",
            "formula": "speed * 2",
            "inputs": [inline("speed", vec![0.0, 1.0, 2.0], vec![0.0, 10.0, 20.0], "linear")],
            "raster": {"mode": "merge"},
        }));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["name"], "power");
        assert_eq!(payload["t"], json!([0.0, 1.0, 2.0]));
        assert_eq!(payload["v"], json!([0.0, 20.0, 40.0]));
    }

    #[test]
    fn a_fixed_raster_uses_the_requested_step() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0, 1.0], vec![0.0, 4.0], "linear")],
            "raster": {"mode": "fixed", "dt": 0.5},
        }));
        assert_eq!(payload["t"], json!([0.0, 0.5, 1.0]), "{payload}");
        assert_eq!(payload["v"], json!([0.0, 2.0, 4.0]));
    }

    #[test]
    fn a_same_as_raster_takes_the_named_inputs_time_base() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a + b",
            "inputs": [
                inline("a", vec![0.0, 2.0], vec![1.0, 3.0], "step"),
                inline("b", vec![0.0, 1.0, 2.0], vec![0.0, 0.0, 0.0], "step"),
            ],
            // Case-insensitively, as the Java looked it up.
            "raster": {"mode": "sameAs", "sameAs": "A"},
        }));
        assert_eq!(payload["t"], json!([0.0, 2.0]), "{payload}");
    }

    #[test]
    fn an_unknown_raster_mode_is_named_not_guessed() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0], vec![1.0], "step")],
            "raster": {"mode": "sliding"},
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("sliding"));
    }

    /// The one error the frontend handles specially: it offers `suggestedDt`
    /// as a button, so the numbers have to be on the payload.
    #[test]
    fn a_raster_over_the_cap_carries_a_dt_that_fits() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0, 1000.0], vec![0.0, 1.0], "linear")],
            "raster": {"mode": "fixed", "dt": 1e-6},
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "RASTER_CAP_EXCEEDED");
        assert_eq!(payload["error"]["cap"], json!(MAX_RASTER));
        let actual = payload["error"]["actualPoints"].as_u64().expect("points");
        assert!(actual > u64::from(MAX_RASTER), "{payload}");
        let dt = payload["error"]["suggestedDt"].as_f64().expect("dt");
        // The suggestion must verifiably fit, or it is not a fix.
        assert!(
            (1000.0f64 / dt).floor() + 1.0 <= f64::from(MAX_RASTER),
            "{payload}"
        );
    }

    /// A call-bearing formula gets a tenth of the budget, so the same raster
    /// that passes above is refused here — and the cap in the payload says so.
    #[test]
    fn a_call_bearing_formula_gets_the_smaller_cap() {
        let payload = calc(json!({
            "name": "x",
            "formula": "sin(a)",
            "inputs": [inline("a", vec![0.0, 1000.0], vec![0.0, 1.0], "linear")],
            "raster": {"mode": "fixed", "dt": 0.001},
        }));
        assert_eq!(payload["error"]["code"], "RASTER_CAP_EXCEEDED", "{payload}");
        assert_eq!(payload["error"]["cap"], json!(MAX_RASTER_WITH_CALLS));
    }

    #[test]
    fn a_broken_formula_reports_the_column() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a +",
            "inputs": [inline("a", vec![0.0], vec![1.0], "step")],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "FORMULA_ERROR");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .starts_with("Formula error:"));
    }

    #[test]
    fn an_empty_formula_and_an_unbound_request_are_both_refused() {
        let empty = calc(json!({"name": "x", "formula": "   ", "inputs": []}));
        assert_eq!(empty["ok"], json!(false), "{empty}");
        assert!(empty["error"]["message"]
            .as_str()
            .expect("message")
            .contains("formula is empty"));

        let unbound = calc(json!({"name": "x", "formula": "1 + 1", "inputs": []}));
        assert_eq!(unbound["ok"], json!(false), "{unbound}");
        assert!(unbound["error"]["message"]
            .as_str()
            .expect("message")
            .contains("at least one input"));
    }

    /// The Java silently read any unrecognised interp as `linear`, which for a
    /// valve position invents openings that never happened.
    #[test]
    fn a_misspelled_interpolation_mode_is_refused_not_defaulted() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("a", vec![0.0, 1.0], vec![0.0, 1.0], "stpe")],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("stpe"));
    }

    #[test]
    fn interpolation_modes_are_read_case_insensitively_and_default_to_linear() {
        assert_eq!(parse_interp(Some("STEP"), "a"), Ok(Interp::Step));
        assert_eq!(parse_interp(Some(" Linear "), "a"), Ok(Interp::Linear));
        assert_eq!(parse_interp(None, "a"), Ok(Interp::Linear));
        assert_eq!(parse_interp(Some(""), "a"), Ok(Interp::Linear));
        assert!(parse_interp(Some("nearest"), "a").is_err());
    }

    /// `step` holds; `linear` blends. If the boundary passed the wrong mode
    /// through, these two would agree.
    #[test]
    fn the_interpolation_mode_reaches_the_evaluator() {
        let request = |interp: &str| {
            json!({
                "name": "x",
                "formula": "a",
                "inputs": [inline("a", vec![0.0, 2.0], vec![0.0, 10.0], interp)],
                "raster": {"mode": "fixed", "dt": 1.0},
            })
        };
        assert_eq!(calc(request("step"))["v"], json!([0.0, 0.0, 10.0]));
        assert_eq!(calc(request("linear"))["v"], json!([0.0, 5.0, 10.0]));
    }

    /// `JSON.stringify(NaN)` is `null`, so a browser-built series with a gap
    /// arrives with holes. Reading them as NaN keeps the gap a gap; rejecting
    /// them would refuse every imported CSV channel that has one.
    #[test]
    fn a_null_in_an_inline_series_is_a_gap_not_a_parse_failure() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{
                "var": "a",
                "interp": "linear",
                "inline": {"t": [0.0, 1.0, 2.0], "v": [0.0, Value::Null, 2.0]},
            }],
            "raster": {"mode": "merge"},
        }));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        // The gap survives evaluation and comes back as null, not as 0.
        assert_eq!(payload["v"], json!([0.0, Value::Null, 2.0]));
    }

    #[test]
    fn a_ragged_inline_series_is_refused_by_name() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{"var": "a", "interp": "step", "inline": {"t": [0.0, 1.0], "v": [0.0]}}],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("\"a\""));
    }

    /// Two inputs that differ only in case are one frees variable, so binding
    /// both is ambiguous — `calc::evaluate`'s own guard, reached because this
    /// boundary does *not* lowercase the keys as the Java did.
    #[test]
    fn inputs_that_collide_are_refused_rather_than_silently_dropped() {
        let ambiguous = calc(json!({
            "name": "x",
            "formula": "speed",
            "inputs": [
                inline("Speed", vec![0.0, 1.0], vec![0.0, 1.0], "linear"),
                inline("speed", vec![0.0, 1.0], vec![5.0, 6.0], "linear"),
            ],
        }));
        assert_eq!(ambiguous["ok"], json!(false), "{ambiguous}");

        let duplicated = calc(json!({
            "name": "x",
            "formula": "speed",
            "inputs": [
                inline("speed", vec![0.0, 1.0], vec![0.0, 1.0], "linear"),
                inline("speed", vec![0.0, 1.0], vec![5.0, 6.0], "linear"),
            ],
        }));
        assert_eq!(duplicated["ok"], json!(false), "{duplicated}");
        assert!(duplicated["error"]["message"]
            .as_str()
            .expect("message")
            .contains("twice"));
    }

    /// The whole point of the port: a calc input can be a channel of a file
    /// that is open in this tab.
    #[test]
    fn a_calc_input_can_name_an_open_channel() {
        let id = register("drive.mf4", 1, ramp(11));
        let payload = calc(json!({
            "name": "double",
            "formula": "speed * 2",
            "inputs": [{
                "var": "speed",
                "interp": "linear",
                "measurementId": id,
                "group": 0,
                "channel": "speed",
            }],
            "raster": {"mode": "merge"},
        }));
        assert_eq!(payload["ok"], json!(true), "{payload}");
        assert_eq!(payload["t"].as_array().expect("t").len(), 11);
        assert_eq!(numbers(&payload["v"])[10], 20.0);
    }

    #[test]
    fn a_calc_input_naming_a_closed_file_says_which_input() {
        let payload = calc(json!({
            "name": "x",
            "formula": "speed",
            "inputs": [{"var": "speed", "measurementId": "m404", "channel": "speed"}],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "CHANNEL_NOT_FOUND");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(
            message.contains("speed") && message.contains("m404"),
            "{payload}"
        );
    }

    #[test]
    fn an_input_binding_neither_a_series_nor_a_channel_is_refused() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [{"var": "a", "interp": "step"}],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("neither"));
    }

    #[test]
    fn an_input_without_a_variable_name_is_refused() {
        let payload = calc(json!({
            "name": "x",
            "formula": "a",
            "inputs": [inline("  ", vec![0.0], vec![1.0], "step")],
        }));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(payload["error"]["message"]
            .as_str()
            .expect("message")
            .contains("missing its variable name"));
    }

    /// A property call is the differentiator — the same backend a document
    /// uses, per sample, over measured data.
    #[test]
    fn a_property_call_in_a_formula_reaches_the_property_backend() {
        let payload = calc(json!({
            "name": "h",
            "formula": "enthalpy(Water, T = t_k, P = 101325)",
            "inputs": [inline("t_k", vec![0.0, 1.0], vec![300.0, 300.0], "step")],
            "raster": {"mode": "merge"},
        }));
        // The tabulated backend may decline a point, but it must never trap and
        // must never fabricate: either a number or a typed failure.
        if payload["ok"] == json!(true) {
            let v = numbers(&payload["v"]);
            assert_eq!(v.len(), 2, "{payload}");
        } else {
            assert!(payload["error"]["message"].is_string(), "{payload}");
        }
    }

    // ── the span used for the suggested dt ──────────────────────────────────

    /// Rust's `f64::min` discards NaN, which would hand back a plausible `dt`
    /// for a span nobody knows.
    #[test]
    fn a_nan_time_poisons_the_span_rather_than_being_ignored() {
        let mut inputs = BTreeMap::new();
        inputs.insert(
            "a".to_string(),
            SampledSeries::new(vec![f64::NAN, 1.0], vec![0.0, 1.0], Interp::Linear),
        );
        let (t0, _) = span_of(&inputs);
        assert!(t0.is_nan());
        assert_eq!(java_min(0.0, -0.0), -0.0);
        assert_eq!(java_max(0.0, -0.0), 0.0);
        assert!(java_max(f64::NAN, 0.0).is_nan());
    }

    // ── adversarial: hostile request bodies ─────────────────────────────────
    //
    // The four exports take strings and bytes straight off a `postMessage`.
    // Whatever arrives, the answer is one of the two envelopes — never a trap,
    // because `panic = "abort"` on the profile that ships and the tab holds the
    // only copy of the user's recording.

    /// Every reply is parseable JSON carrying a boolean `ok`, and a failure
    /// always carries a `code` the frontend switches on and a human `message`.
    fn envelope(payload: &str) -> Value {
        let value: Value = serde_json::from_str(payload)
            .unwrap_or_else(|e| panic!("reply is not JSON ({e}): {payload}"));
        match value["ok"].as_bool() {
            Some(true) => {}
            Some(false) => {
                assert!(value["error"]["code"].is_string(), "{value}");
                assert!(value["error"]["message"].is_string(), "{value}");
            }
            None => panic!("reply has no boolean `ok`: {value}"),
        }
        value
    }

    /// Bodies that are not the shape the boundary declares: wrong types, wrong
    /// containers, absurd numbers, and text that is not JSON at all.
    #[test]
    fn no_malformed_request_body_escapes_the_envelope() {
        const BODIES: &[&str] = &[
            "",
            " ",
            "null",
            "[]",
            "0",
            "\"\"",
            "true",
            "{",
            "{}",
            "{\"measurementId\": 7}",
            "{\"measurementId\": null}",
            "{\"group\": -1}",
            "{\"group\": 1e400}",
            "{\"group\": 18446744073709551615}",
            "{\"group\": 184467440737095516150000}",
            "{\"maxPoints\": -5}",
            "{\"maxPoints\": 4294967296}",
            "{\"maxPoints\": 1.5}",
            "{\"from\": \"0\"}",
            "{\"from\": 1e400, \"to\": -1e400}",
            "{\"channel\": []}",
            "{\"inputs\": {}}",
            "{\"inputs\": [null]}",
            "{\"inputs\": [{\"var\": 3}]}",
            "{\"inputs\": [{\"inline\": {\"t\": [\"x\"], \"v\": [1]}}]}",
            // A JSON number outside `f64`: it must not become a plausible time.
            "{\"formula\": \"x\", \"inputs\": [{\"var\": \"x\", \"inline\": \
             {\"t\": [0, 1e400], \"v\": [1, 2]}}]}",
            "{\"formula\": \"x\", \"inputs\": [{\"var\": \"x\", \"inline\": \
             {\"t\": [0, 1], \"v\": [1, -1e400]}}], \"raster\": {\"mode\": \"fixed\", \"dt\": 1e-400}}",
            "{\"raster\": \"merge\"}",
            "{\"raster\": {\"mode\": 3}}",
            "{\"formula\": 12}",
            // Deep nesting: serde_json's own recursion limit has to hold, not
            // ours, because a stack overflow here is an abort like any other.
            "{\"inputs\": [{\"inline\": {\"t\": [[[[[[[[[[1]]]]]]]]]]}}]}",
        ];
        for body in BODIES {
            envelope(&measurement_channel_window(body));
            envelope(&measurement_calc(body));
        }
        // A ten-thousand-deep JSON array, which is the shape that overflows a
        // recursive-descent parser rather than merely failing one.
        let bomb = format!("{}1{}", "[".repeat(10_000), "]".repeat(10_000));
        envelope(&measurement_calc(&bomb));
        envelope(&measurement_channel_window(&bomb));
        envelope(&measurement_calc(&format!("{{\"raster\": {bomb}}}")));
    }

    /// Ids and names are opaque strings from the caller. None of them may reach
    /// a slice index, and all of them come back in the diagnostic verbatim.
    #[test]
    fn hostile_ids_and_channel_names_are_quoted_not_indexed() {
        let id = register("drive.mf4", 1, ramp(10));
        for name in [
            "",
            " ",
            "\0",
            "speed\u{0}",
            "SPEED",
            "🙂",
            "\u{202e}speed",
            &"x".repeat(100_000),
        ] {
            envelope(&measurement_channel_window(
                &json!({"measurementId": &id, "group": 0, "channel": name}).to_string(),
            ));
            envelope(&measurement_channel_window(
                &json!({"measurementId": name, "group": 0, "channel": "speed"}).to_string(),
            ));
            // `close` returns nothing; it must simply not trap.
            measurement_close(name);
        }
        // The real channel still reads afterwards.
        assert_eq!(window(&id, "speed", None, None, 100)["ok"], json!(true));
    }

    /// Ranges the UI can generate by dragging past the ends of a recording, and
    /// ranges no UI generates at all.
    #[test]
    fn every_window_range_answers_within_the_channel() {
        let id = register("drive.mf4", 1, ramp(1000));
        let edges = [
            None,
            Some(f64::MIN),
            Some(-1.0),
            Some(0.0),
            Some(-0.0),
            Some(50.0),
            Some(99.9),
            Some(1e300),
            Some(f64::MAX),
        ];
        for from in edges {
            for to in edges {
                for max in [0u32, 1, 2, 3, 2400, 20_000, u32::MAX] {
                    let payload = window(&id, "speed", from, to, max);
                    assert_eq!(payload["ok"], json!(true), "{from:?}..{to:?} @{max}");
                    let t = numbers(&payload["t"]);
                    assert_eq!(payload["totalSamples"], json!(1000));
                    assert!(
                        t.len() <= MAX_MAX_POINTS as usize,
                        "{from:?}..{to:?} @{max} -> {} points",
                        t.len()
                    );
                    // Every reported time is a sample the channel really has,
                    // and the sequence never runs backwards. The tolerance is
                    // not slop in the boundary: it is the 1 ULP `serde_json`
                    // drops when *this test* re-parses the reply, which is the
                    // finding pinned by
                    // `a_json_round_trip_moves_a_timestamp_by_one_ulp`.
                    let master: Vec<f64> = (0..1000).map(|i| f64::from(i) * 0.1).collect();
                    for x in &t {
                        assert!(
                            master.iter().any(|m| (m - x).abs() <= 1e-12),
                            "{from:?}..{to:?} @{max} -> stray time {x:?}"
                        );
                    }
                    assert!(
                        t.windows(2).all(|w| w[0] < w[1]),
                        "{from:?}..{to:?} @{max} -> out of order"
                    );
                }
            }
        }
    }

    /// A recorded time master that does not ascend.
    ///
    /// `slice_window` derives both range ends from `lower_bound`, which assumes
    /// an ordered array; on a scrambled one it returns an arbitrary index, and
    /// the arithmetic that follows — `saturating_sub(1)`, `min(n - 1)`, the
    /// inverted-range escape, and `min_max`'s own clamp — is the only thing
    /// keeping the slices inside the channel. Nothing here can be *right* for a
    /// corrupt master; it has to be in bounds and it has to be samples the file
    /// actually holds.
    #[test]
    fn a_scrambled_time_master_still_windows_inside_the_channel() {
        let time = vec![0.0, 9.0, 1.0, f64::NAN, -4.0, 2.0, f64::INFINITY, 3.0];
        let values = vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];
        let mut channels = BTreeMap::new();
        channels.insert(
            (0, "speed".to_string()),
            ChannelData {
                time: time.clone(),
                values: values.clone(),
            },
        );
        let id = register(
            "corrupt.mf4",
            1,
            FakeSource {
                metadata: MeasurementMetadata {
                    groups: vec![GroupInfo {
                        index: 0,
                        name: "g".to_string(),
                        records: 8,
                        channels: vec![ChannelInfo {
                            name: "speed".to_string(),
                            unit: None,
                            time_master: false,
                            kind: ChannelKind::Analog,
                        }],
                    }],
                },
                channels,
            },
        );

        for from in [None, Some(-1e9), Some(0.0), Some(2.5), Some(1e9)] {
            for to in [None, Some(-1e9), Some(0.0), Some(2.5), Some(1e9)] {
                for max in [2u32, 3, 8, 2400] {
                    let payload = window(&id, "speed", from, to, max);
                    assert_eq!(payload["ok"], json!(true), "{payload}");
                    assert_eq!(payload["totalSamples"], json!(8));
                    // A gap crosses the wire as null, so read the raw array.
                    let t = payload["t"].as_array().expect("t");
                    assert!(t.len() <= 8, "{from:?}..{to:?} @{max}: {} points", t.len());
                    for x in t {
                        let matches = match x.as_f64() {
                            // `null` is a non-finite sample: the master really
                            // does hold a NaN and an infinity.
                            None => true,
                            Some(x) => time.contains(&x),
                        };
                        assert!(matches, "{from:?}..{to:?} @{max} -> stray time {x}");
                    }
                }
            }
        }
    }

    /// A calc request whose inline series are degenerate in every way the wire
    /// allows: empty, all-null, single-point, non-finite, and enormous in
    /// magnitude. Each answers; none traps.
    #[test]
    fn degenerate_inline_series_answer_rather_than_trap() {
        let series: &[(&str, Value)] = &[
            ("empty", json!({"t": [], "v": []})),
            ("one", json!({"t": [0.0], "v": [1.0]})),
            ("all null", json!({"t": [null, null], "v": [null, null]})),
            ("null times", json!({"t": [null, 1.0], "v": [1.0, 2.0]})),
            ("huge", json!({"t": [-1e308, 1e308], "v": [1.0, 2.0]})),
            ("negative span", json!({"t": [5.0, 0.0], "v": [1.0, 2.0]})),
            (
                "repeated",
                json!({"t": [0.0, 0.0, 0.0], "v": [1.0, 2.0, 3.0]}),
            ),
        ];
        let rasters: &[Value] = &[
            json!(null),
            json!({"mode": "merge"}),
            json!({"mode": "fixed", "dt": 0.1}),
            json!({"mode": "fixed", "dt": 1e-300}),
            json!({"mode": "fixed", "dt": 1e300}),
            json!({"mode": "sameAs", "sameAs": "x"}),
            json!({"mode": "sameAs", "sameAs": "nope"}),
        ];
        for (label, inline) in series {
            for raster in rasters {
                for formula in ["x + 1", "movavg(x, 1)", "integral(x)", "delay(x, 1)"] {
                    let payload = envelope(&measurement_calc(
                        &json!({
                            "name": "out",
                            "formula": formula,
                            "inputs": [{"var": "x", "interp": "linear", "inline": inline}],
                            "raster": raster,
                        })
                        .to_string(),
                    ));
                    if payload["ok"] == json!(true) {
                        assert_eq!(
                            payload["t"].as_array().map(Vec::len),
                            payload["v"].as_array().map(Vec::len),
                            "{label} / {raster} / {formula}: t and v disagree"
                        );
                    }
                }
            }
        }
    }

    /// `measurement_open` over bytes that are not a recording — every prefix of
    /// a header, and random noise behind a valid identifier.
    #[test]
    fn no_byte_string_opens_into_a_trap() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..400 {
            let len = (next() % 700) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| (next() & 0xFF) as u8).collect();
            if round % 2 == 0 && len >= 8 {
                bytes[..8].copy_from_slice(b"MDF     ");
            }
            let payload = envelope(&measurement_open(bytes, "drop.mf4"));
            // Nothing hostile ever parses, so nothing is retained.
            assert_eq!(payload["ok"], json!(false), "round {round}");
        }
        assert_eq!(with_registry(|r| r.retained), 0);
    }

    // ── the JSON number pipe is not round-trip exact ────────────────────────

    /// **Open defect, not fixed here — the fix is a Cargo feature.**
    ///
    /// `serde_json`'s default float parser is the fast one, and it is documented
    /// to be accurate only to within 1 ULP. Its `float_roundtrip` feature buys
    /// the exact parser and is **off** in this workspace, so a number this
    /// module *emits* correctly (`ryu` is exact) does not survive being read
    /// back: `1.4000000000000001` — the f64 nearest `14 × 0.1`, and exactly what
    /// `JSON.stringify` produces for it — deserialises as `1.4`.
    ///
    /// This is a boundary-wide exposure (every `f64` in every request body), but
    /// measurement is where it does visible damage, because two of this
    /// module's rules are **exact equality** on `f64`:
    ///
    /// * `SampledSeries::at` returns a sample verbatim on an exact time hit and
    ///   interpolates otherwise. One ULP turns a hit into a blend, and on a
    ///   `step` channel a blend reports the *previous* sample — the valve
    ///   position before it opened.
    /// * `raster::union` dedupes with IEEE `==`. One ULP turns one shared
    ///   timestamp into two raster points.
    ///
    /// [`a_time_that_arrives_as_json_no_longer_merges_with_the_same_time_from_a_file`]
    /// drives both through the public entry points. The remedy is one word in
    /// the workspace manifest —
    /// `serde_json = { version = "1", features = ["float_roundtrip"] }` — which
    /// is outside this module, so it is reported rather than applied.
    ///
    /// **Both halves of that report are now measured.** With the feature on,
    /// this test and its companion *fail* — which is the point of writing them
    /// this way round, and is the check that the feature is the actual fix
    /// rather than a plausible one. The price is +19.2 KiB of wasm against
    /// 129.8 KiB of headroom. So the open question is not "does it work" or
    /// "can we afford it" — both are answered — but who rewrites these two
    /// tests into their positive form when it lands.
    #[test]
    fn a_json_round_trip_moves_a_timestamp_by_one_ulp() {
        let exact = 14.0_f64 * 0.1;
        let text = serde_json::to_string(&exact).expect("ryu is exact");
        assert_eq!(text, "1.4000000000000001", "the emitted digits are right");
        let back: f64 = serde_json::from_str(&text).expect("and re-read");
        assert_ne!(
            back.to_bits(),
            exact.to_bits(),
            "if this passes, the feature landed"
        );
        assert_eq!(back, 1.4);
    }

    /// The damage the ULP does, driven through `measurement_calc` itself.
    ///
    /// One input is a channel of an open recording, whose time master never
    /// touches JSON. The other is the *same* time base sent inline, exactly as
    /// `JSON.stringify` would write it. They should merge to three raster
    /// points; they merge to four, and at the spurious point the step channel
    /// reports the value from before the sample rather than the one at it.
    #[test]
    fn a_time_that_arrives_as_json_no_longer_merges_with_the_same_time_from_a_file() {
        let master = vec![0.0, 14.0 * 0.1, 3.0];
        let mut channels = BTreeMap::new();
        channels.insert(
            (0, "a".to_string()),
            ChannelData {
                time: master.clone(),
                values: vec![10.0, 20.0, 30.0],
            },
        );
        let id = register(
            "drive.mf4",
            1,
            FakeSource {
                metadata: MeasurementMetadata {
                    groups: vec![GroupInfo {
                        index: 0,
                        name: "g".to_string(),
                        records: 3,
                        channels: vec![ChannelInfo {
                            name: "a".to_string(),
                            unit: None,
                            time_master: false,
                            kind: ChannelKind::Analog,
                        }],
                    }],
                },
                channels,
            },
        );

        // The identical times, as the browser would send them.
        let inline_text = serde_json::to_string(&master).expect("ryu");
        assert!(inline_text.contains("1.4000000000000001"), "{inline_text}");
        let request = format!(
            "{{\"name\":\"out\",\"formula\":\"a\",\"raster\":{{\"mode\":\"merge\"}},\
             \"inputs\":[\
             {{\"var\":\"a\",\"interp\":\"step\",\"measurementId\":\"{id}\",\
               \"group\":0,\"channel\":\"a\"}},\
             {{\"var\":\"b\",\"interp\":\"step\",\"inline\":\
               {{\"t\":{inline_text},\"v\":[1,2,3]}}}}]}}"
        );
        let payload = envelope(&measurement_calc(&request));
        assert_eq!(payload["ok"], json!(true), "{payload}");

        let t = numbers(&payload["t"]);
        let v = numbers(&payload["v"]);
        // Three distinct instants were recorded; four come back.
        assert_eq!(t.len(), 4, "the ULP split one timestamp in two: {t:?}");
        // At the earlier of the two, channel `a` has not reached its sample, so
        // the step hold reports the previous one.
        assert_eq!(v, vec![10.0, 10.0, 20.0, 30.0], "{t:?}");
    }

    // ── adversarial: the inputs are read before the raster is capped ────────

    /// A source whose metadata *claims* `records` samples while its channel
    /// holds four, so the budget can be driven at a size no test could
    /// allocate — the same trick `register`'s claimed `size` plays for the
    /// eviction budget.
    ///
    /// The claim is the honest basis for the guard, not a test convenience:
    /// `Mdf4Source::channel` truncates its output to exactly the `records` its
    /// metadata reports, so for a real recording the two agree by construction.
    fn claims(records: u64) -> FakeSource {
        let mut channels = BTreeMap::new();
        channels.insert(
            (0, "speed".to_string()),
            ChannelData {
                time: vec![0.0, 1.0, 2.0, 3.0],
                values: vec![10.0, 20.0, 30.0, 40.0],
            },
        );
        FakeSource {
            metadata: MeasurementMetadata {
                groups: vec![GroupInfo {
                    index: 0,
                    name: "huge".to_string(),
                    records,
                    channels: vec![ChannelInfo {
                        name: "speed".to_string(),
                        unit: None,
                        time_master: false,
                        kind: ChannelKind::Analog,
                    }],
                }],
            },
            channels,
        }
    }

    /// Ten variables naming one 16 M-sample channel is a few hundred bytes of
    /// request and 2.7 GB of `SampledSeries`, held all at once, *before* the
    /// million-point raster cap gets a say. On wasm32 that is an allocator trap
    /// and `panic = "abort"`, so nothing downstream renders it.
    ///
    /// The refusal has to land **before** the reads, which is why the budget is
    /// claimed from the header count rather than measured after the fact.
    #[test]
    fn many_inputs_naming_one_huge_channel_are_refused_before_they_are_read() {
        let id = register("huge.mf4", 1, claims(4_000_000));
        let inputs: Vec<Value> = ('a'..='j')
            .map(|c| {
                json!({"var": c.to_string(), "measurementId": &id, "group": 0, "channel": "speed"})
            })
            .collect();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "a + b", "inputs": inputs}).to_string(),
        ));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(
            message.contains(&MAX_INPUT_SAMPLES.to_string()),
            "{message}"
        );
        assert!(message.contains("Bind fewer signals"), "{message}");
    }

    /// The guard must leave the case merging exists for alone. The raster
    /// module's own example — ten channels on one fast master, ten times the
    /// raw samples merging to one raster — has to pass, and so must a single
    /// channel at the reader's own per-channel ceiling.
    #[test]
    fn ten_ordinary_channels_still_merge() {
        let id = register("drive.mf4", 1, claims(1_000_000));
        let inputs: Vec<Value> = ('a'..='j')
            .map(|c| {
                json!({"var": c.to_string(), "measurementId": &id, "group": 0, "channel": "speed"})
            })
            .collect();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "a + b", "inputs": inputs}).to_string(),
        ));
        assert_eq!(payload["ok"], json!(true), "{payload}");

        let mut budget = SampleBudget::default();
        budget
            .claim("one", MAX_INPUT_SAMPLES)
            .expect("the ceiling itself fits");
        assert!(budget.claim("two", 1).is_err(), "but not one sample more");
    }

    /// The other half of the same amplification: not how many *samples* the
    /// inputs hold, but how many *inputs* there are.
    ///
    /// `calc::evaluate` costs `raster × inputs`, and one-point inline series
    /// make the raster as long as the input list — so the whole call is
    /// quadratic in the request body. Measured in release before the cap:
    /// 2 000 inputs 0.15 s, 4 000 0.58 s, 8 000 2.3 s, 16 000 12.5 s —
    /// extrapolating to about 50 s at 32 000, from 1.8 MB of JSON. The sample
    /// budget cannot see it, because a million one-point series is only a
    /// million samples.
    ///
    /// **Each input needs its own time**, and that is not cosmetic: the raster
    /// is the *union* of the input time bases, so a thousand series that all
    /// sample `t = 0` dedupe to a one-point raster and the product `raster ×
    /// inputs` never grows. Written with `"t": [0.0]` throughout, the 40 000-
    /// input case below returned in about 180 ms with the cap removed, and its
    /// timing assertion proved nothing.
    #[test]
    fn a_request_binding_more_inputs_than_a_formula_can_name_is_refused() {
        let one =
            |i: usize| json!({"var": format!("v{i}"), "inline": {"t": [i as f64], "v": [1.0]}});

        let at_cap: Vec<Value> = (0..MAX_INPUTS).map(one).collect();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "v0 + v1", "inputs": at_cap}).to_string(),
        ));
        assert_eq!(payload["ok"], json!(true), "the cap itself is allowed");

        let over: Vec<Value> = (0..=MAX_INPUTS).map(one).collect();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "v0 + v1", "inputs": over}).to_string(),
        ));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(message.contains(&(MAX_INPUTS + 1).to_string()), "{message}");
        assert!(message.contains(&MAX_INPUTS.to_string()), "{message}");

        // And the refusal lands before anything is read: 40 000 inputs is the
        // shape that took 52 s, and it has to come back immediately.
        let bomb: Vec<Value> = (0..40_000).map(one).collect();
        let start = std::time::Instant::now();
        let payload = envelope(&measurement_calc(
            &json!({"name": "out", "formula": "v0 + v1", "inputs": bomb}).to_string(),
        ));
        let elapsed = start.elapsed();
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "40 000 inputs took {elapsed:?}"
        );
    }

    /// Both caps above are satisfiable *at the same time*, and their product is
    /// the memory. 128 inputs is exactly [`MAX_INPUTS`]; a fixed raster over
    /// their span is exactly [`MAX_RASTER`]; each one costs `calc::evaluate` a
    /// full-length column, held all at once.
    ///
    /// Measured through this entry point with a counting global allocator: this
    /// **5 604-byte** request body peaked at **1 044 MB** — a 186 000×
    /// amplification, past what a tab will grow linear memory to, and the
    /// allocation site is a `collect()`, so on wasm32 it is an abort rather than
    /// this diagnostic. Neither cap sees it: `MAX_INPUT_SAMPLES` counts input
    /// samples and 128 one-point series is 128 of them, and `MAX_RASTER` counts
    /// the raster and a million is what it allows. The guard is
    /// `calc::MAX_INPUT_COLUMN_SAMPLES`, in the module that does the allocating; the
    /// same call now peaks at 132 MB.
    #[test]
    fn the_widest_legal_input_list_on_the_longest_legal_raster_is_refused() {
        // One input pins t = 0 and another t = 999 999, so `span_of` is the
        // whole million and `dt = 1` lands exactly on the raster cap.
        let inputs: Vec<Value> = (0..MAX_INPUTS)
            .map(|i| {
                let t = if i == 1 { 999_999.0 } else { 0.0 };
                json!({"var": format!("v{i}"), "inline": {"t": [t], "v": [1.0]}})
            })
            .collect();
        let body = json!({
            "name": "out",
            "formula": "v0 + 1",
            "inputs": inputs,
            "raster": {"mode": "fixed", "dt": 1.0},
        })
        .to_string();
        assert!(body.len() < 8_192, "the request is small: {} B", body.len());

        let payload = envelope(&measurement_calc(&body));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        assert_eq!(payload["error"]["code"], "FORMULA_ERROR");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(message.contains("bind fewer signals"), "{message}");

        // Narrower, same raster: still the ordinary case, still answered.
        let inputs: Vec<Value> = (0..4)
            .map(|i| {
                let t = if i == 1 { 999_999.0 } else { 0.0 };
                json!({"var": format!("v{i}"), "inline": {"t": [t], "v": [1.0]}})
            })
            .collect();
        let payload = envelope(&measurement_calc(
            &json!({
                "name": "out",
                "formula": "v0 + 1",
                "inputs": inputs,
                "raster": {"mode": "fixed", "dt": 1.0},
            })
            .to_string(),
        ));
        assert_eq!(
            payload["ok"],
            json!(true),
            "four inputs must still evaluate"
        );
        assert_eq!(
            payload["t"].as_array().expect("times").len(),
            MAX_RASTER as usize
        );
    }

    /// An inline series counts against the **same** budget as a channel, so a
    /// request cannot slip past the ceiling by splitting itself across the two
    /// kinds of input.
    ///
    /// Driven through `measurement_calc` rather than through `SampleBudget`
    /// directly, because the defect this guards is a missing *call*, not a
    /// wrong sum: deleting `budget.claim(var, inline.t.len())` from
    /// `resolve_inputs` leaves every assertion in
    /// [`the_input_budget_counts_inline_series_and_cannot_overflow`] passing.
    /// The channel is claimed first and lands exactly *on* the ceiling, so only
    /// the inline claim can push it over — and the message has to name the
    /// inline variable to prove which claim fired.
    #[test]
    fn an_inline_series_shares_the_budget_with_a_channel() {
        let id = register("edge.mf4", 1, claims(MAX_INPUT_SAMPLES));
        let payload = envelope(&measurement_calc(
            &json!({
                "name": "out",
                "formula": "a + b",
                "inputs": [
                    {"var": "a", "measurementId": &id, "group": 0, "channel": "speed"},
                    {"var": "b", "inline": {"t": [0.0, 1.0], "v": [1.0, 2.0]}},
                ],
            })
            .to_string(),
        ));
        assert_eq!(payload["ok"], json!(false), "{payload}");
        let message = payload["error"]["message"].as_str().expect("message");
        assert!(message.contains('b'), "{message}");
        assert!(
            message.contains(&MAX_INPUT_SAMPLES.to_string()),
            "{message}"
        );

        // …and the channel alone, exactly at the ceiling, is still allowed —
        // otherwise the assertion above would pass for the wrong reason.
        let payload = envelope(&measurement_calc(
            &json!({
                "name": "out",
                "formula": "a",
                "inputs": [{"var": "a", "measurementId": &id, "group": 0, "channel": "speed"}],
            })
            .to_string(),
        ));
        assert_eq!(payload["ok"], json!(true), "{payload}");
    }

    /// An inline series counts against the same budget, and the claim cannot
    /// wrap on the way to the check.
    #[test]
    fn the_input_budget_counts_inline_series_and_cannot_overflow() {
        let mut budget = SampleBudget::default();
        assert!(budget.claim("a", u64::MAX).is_err());
        assert!(
            budget.claim("b", u64::MAX).is_err(),
            "saturating, not wrapping"
        );

        let payload = envelope(&measurement_calc(
            &json!({
                "name": "out",
                "formula": "x",
                "inputs": [{"var": "x", "inline": {"t": [0.0, 1.0], "v": [1.0, 2.0]}}],
            })
            .to_string(),
        ));
        assert_eq!(
            payload["ok"],
            json!(true),
            "an ordinary series is unaffected"
        );
    }

    /// The oversize refusal happens before anything is parsed or retained, at a
    /// size no test could allocate.
    #[test]
    fn a_file_above_the_budget_is_refused_by_arithmetic_not_by_trying() {
        assert!(refuse_oversize(RETAINED_BYTES_BUDGET).is_ok());
        let message = refuse_oversize(RETAINED_BYTES_BUDGET + 1)
            .unwrap_err()
            .to_string();
        assert!(message.contains("shorter time range"), "{message}");
        assert!(refuse_oversize(u64::MAX).is_err());
    }
}
