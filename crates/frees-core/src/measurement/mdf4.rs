//! ASAM MDF4 (`.mf4`) reading — from bytes, in the tab.
//!
//! Port of `../frEES/backend/core/src/main/java/com/frees/backend/measurement/Mf4Parser.java`
//! (254 LOC), which wrapped **mdf4j**. mdf4j memory-maps the file and streams
//! records lazily; there is no file to map in a browser, so the backing crate
//! here is [`mf4_rs`] and the recording is the `Vec<u8>` the caller already
//! holds. [`Mdf4Source`] owns those bytes for its whole life — nothing is
//! re-read, and nothing is uploaded.
//!
//! # Format coverage, against the Java baseline
//!
//! `Mf4Parser`'s javadoc names three limits: MDF ≤ 4.20 (deflate `##DZ` fine,
//! 4.30 ZSTD/LZ4 rejected), identity + linear conversions only, sorted files
//! only. This reader matches two of them and **loses one**:
//!
//! | | Java (mdf4j 0.2.0) | here |
//! |---|---|---|
//! | uncompressed `##DT`/`##DV`, `##DL` chains | yes | yes |
//! | **deflate `##DZ`** | **yes** | **no — refused at [`open`]** |
//! | 4.30 ZSTD/LZ4 `##DZ` | no | no |
//! | conversions | identity + linear | + rational, algebraic, table lookups |
//! | value-to-text channels | raw numbers | raw numbers (see [`read_samples`]) |
//! | unsorted (multi-group) data groups | no | no |
//!
//! The `##DZ` row is the real cost of collapsing the Java's
//! `Mf4Parser` → `FallbackMeasurementParser` → `mdf-sidecar` ladder to one
//! rung. mdf4j inflated deflate blocks itself; `mf4-rs` has **no decompressor
//! in its dependency tree at all** — its reader accepts only `##DT`/`##DV`/`##DL`
//! and its own docs say compressed blocks are unsupported. Adding one means a
//! zlib crate in the wasm bundle, which is a budget decision rather than a
//! coding one (the bundle is already over budget — see `docs/status-phase9.md`).
//! Until then a compressed recording must be re-exported uncompressed, and
//! [`open`] says exactly that instead of letting the user discover it one
//! channel at a time.
//!
//! Compression is detected at [`open`] on purpose. Metadata parsing does not
//! touch data blocks, so a `##DZ` file *would* list its channels happily and
//! then fail on every single read — a menu of things that cannot be fetched.
//! One error, at load, naming the remedy, is the better trade.
//!
//! # Robustness
//!
//! A measurement file is the most hostile input in the product: arbitrary bytes
//! from an arbitrary tool, parsed as a graph of self-describing blocks whose
//! lengths and link addresses all come out of the file itself. Two ways to turn
//! that into a **process abort** were found in `mf4-rs` 3.6.0 by fuzzing
//! mutations of a real recording, and both are closed here:
//! [`validate_block_graph`] (a corrupt `##CC` link) and the `cn_cycle_count`
//! plausibility test in [`probe_records`]. Neither could have been caught after
//! the fact — the wasm profile sets `panic = "abort"`.
//!
//! A later adversarial sweep closed three more, all of the same shape: a bound
//! that existed but was **stated in the wrong unit**, so a file could satisfy it
//! and still cost unbounded time or memory.
//!
//! * [`MAX_RECORDS`] was tested against `min(declared, physical)`, but the
//!   number `mf4-rs` hands to `Vec::with_capacity` is the *declared*
//!   `cn_cycle_count`. Since [`probe_records`] bounds a declaration only by the
//!   file's own length, trailing padding buys headroom for free — a 4 MB file
//!   holding 80 samples asked for 67 MB, and at the browser boundary's 512 MiB
//!   file limit the same shape asks for 8 GiB. ([`MeasurementSource::channel`].)
//! * [`MAX_BLOCKS`] bounds how many blocks the pre-flight *visits*, but the
//!   visited list was scanned linearly, so the **cost** was that ceiling
//!   squared: 17.6 s at 160 000 blocks, about eleven minutes at the ceiling.
//!   ([`visit`].)
//! * Nothing at all bounded the sample-storage walk, which `mf4-rs` repeats
//!   once per channel group. Groups and `##DL` blocks are both cheap, and the
//!   cost is their product. ([`check_storage_chain`].)

use super::{
    ChannelData, ChannelInfo, ChannelKind, GroupInfo, MeasurementError, MeasurementMetadata,
    MeasurementSource, Result,
};
use mf4_rs::api::channel::Channel;
use mf4_rs::api::channel_group::ChannelGroup;
use mf4_rs::api::mdf::MDF;
use mf4_rs::blocks::channel_block::ChannelBlock;
use mf4_rs::blocks::common::DataType;
use mf4_rs::blocks::conversion::ConversionType;
use mf4_rs::error::MdfError;
use mf4_rs::parsing::raw_channel::RawChannel;

/// Upper bound on the records a single [`MeasurementSource::channel`] call will
/// materialise.
///
/// A [`ChannelData`] costs 16 bytes per record (a time and a value), so this is
/// a 256 MiB ceiling — roughly 14× the largest fixture the parent's MDF4 spike
/// used. The Java capped at `Integer.MAX_VALUE - 8` because its arrays were
/// `int`-indexed; wasm32 has a far harder limit (the whole linear memory, 4 GiB
/// minus everything already in it) and exceeding it is a **trap**, not an
/// exception. Refusing with a number the user can act on beats taking the tab
/// down.
const MAX_RECORDS: u64 = 16_777_216;

/// How many blocks [`validate_block_graph`] walks before giving up.
///
/// Every visited block sits at a distinct address, so a real file's chain is
/// bounded by its size; this only stops a crafted file from making the walk
/// itself the denial of service. No recording has a million channels.
const MAX_BLOCKS: usize = 1_000_000;

/// How many names a "not found" diagnostic lists before it says "and N more".
const MAX_LISTED: usize = 40;

/// A `.mf4` recording held in memory, parsed down to its block headers.
///
/// Construct with [`open`]. Sample data is *not* decoded until
/// [`MeasurementSource::channel`] asks for it, which is what keeps opening a
/// 100 MB recording cheap.
pub struct Mdf4Source {
    mdf: MDF,
    /// Record count per channel group, in `mdf.channel_groups()` order.
    /// Computed once at [`open`]; see [`probe_records`] for why it is not just
    /// `cn_cycle_count`.
    records: Vec<u64>,
}

/// Deliberately *not* `#[derive]`d: `MDF`'s own `Debug` prints the backing
/// store, which is the entire recording. A hundred megabytes of hex in a panic
/// message helps nobody.
impl core::fmt::Debug for Mdf4Source {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Mdf4Source")
            .field("groups", &self.records.len())
            .field("records", &self.records)
            .finish()
    }
}

/// Parse `bytes` as an MDF4 recording.
///
/// Everything checkable without decoding samples is checked here: the block
/// graph is bounded, every data group's storage is walked (which is what
/// rejects compressed and unsorted files), and each group's record count is
/// established. After this returns `Ok`, [`MeasurementSource::metadata`] cannot
/// fail for structural reasons.
pub fn open(bytes: Vec<u8>) -> Result<Mdf4Source> {
    let file_len = bytes.len() as u64;
    validate_block_graph(&bytes)?;

    let mdf = MDF::from_bytes(bytes).map_err(|e| MeasurementError::Parse(describe(&e)))?;

    let mut records = Vec::new();
    {
        let groups = mdf.channel_groups();
        if groups.is_empty() {
            // Worded as the Java worded it — same remedy, same sentence.
            return Err(MeasurementError::Parse(
                "The MDF4 file contains no channel groups.".to_string(),
            ));
        }
        records.reserve(groups.len());
        for (index, group) in groups.iter().enumerate() {
            records.push(probe_records(group, index, file_len)?);
        }
    }

    Ok(Mdf4Source { mdf, records })
}

impl MeasurementSource for Mdf4Source {
    fn metadata(&self) -> Result<MeasurementMetadata> {
        let groups = self.mdf.channel_groups();
        let mut out = Vec::with_capacity(groups.len());

        for (index, group) in groups.iter().enumerate() {
            let name = group
                .name()
                .map_err(|e| MeasurementError::Parse(describe(&e)))?
                .filter(|n| !n.is_empty())
                // The Java's `getName().orElse("group " + index)`, verbatim.
                .unwrap_or_else(|| format!("group {index}"));

            let channels = group.channels();
            let mut infos = Vec::with_capacity(channels.len());
            for (i, channel) in channels.iter().enumerate() {
                // A *failed* name read means the file's links are corrupt, which
                // is a parse failure (mdf4j threw IOException here too). Only a
                // genuinely absent name block falls back to a positional label.
                let name = channel
                    .name()
                    .map_err(|e| MeasurementError::Parse(describe(&e)))?
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("channel {i}"));
                let unit = channel
                    .unit()
                    .map_err(|e| MeasurementError::Parse(describe(&e)))?
                    .filter(|u| !u.is_empty());
                infos.push(ChannelInfo {
                    name,
                    unit,
                    time_master: is_time_master(channel.block()),
                    kind: kind_of(channel.block()),
                });
            }

            out.push(GroupInfo {
                index,
                name,
                records: self.records[index],
                channels: infos,
            });
        }

        Ok(MeasurementMetadata { groups: out })
    }

    fn channel(&self, group_index: usize, channel_name: &str) -> Result<ChannelData> {
        let groups = self.mdf.channel_groups();
        let group = groups
            .get(group_index)
            .ok_or_else(|| no_such_group(group_index, &groups))?;

        let channels = group.channels();
        let mut names = Vec::with_capacity(channels.len());
        for channel in &channels {
            names.push(
                channel
                    .name()
                    .map_err(|e| MeasurementError::Parse(describe(&e)))?
                    .unwrap_or_default(),
            );
        }

        // The user's own spelling wins outright; the case-insensitive pass is a
        // fallback that can never change which channel an exact match picked.
        // (The Java compared with `String.equals` and had no fallback.)
        let target = names
            .iter()
            .position(|n| n == channel_name)
            .or_else(|| {
                names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(channel_name))
            })
            .ok_or_else(|| no_such_channel(group_index, channel_name, &names))?;

        let master = channels
            .iter()
            .position(|c| is_time_master(c.block()))
            .ok_or_else(|| {
                MeasurementError::Parse(format!(
                    "Channel group {group_index} (\"{}\") has no time master channel, so its \
                     samples cannot be placed on a time axis. Re-export the recording with a \
                     time channel.",
                    group_label(group_index, group)
                ))
            })?;

        // A *virtual* master (cn_type 3) has no bytes in the record: its raw
        // value is the record index, and the conversion turns that into a time.
        // Synthesising it is a dozen lines, but getting it subtly wrong yields a
        // silently wrong time axis, which is worse than a refusal — and there is
        // no fixture in this repo to prove it right on. So: refuse, loudly.
        if channels[master].block().channel_type == 3 {
            return Err(MeasurementError::Parse(format!(
                "Channel group {group_index} (\"{}\") uses a virtual master channel (\"{}\"), \
                 whose timestamps are computed rather than stored. This reader only reads a \
                 stored time master; re-export the recording with an explicit time channel.",
                group_label(group_index, group),
                names[master]
            )));
        }

        // The cap has to be tested against the count `mf4-rs` will *plan* for,
        // which is the group's declared `cn_cycle_count` — its `values_as_f64`
        // opens with `Vec::with_capacity(cycles_nr)` before it decodes a single
        // record. `probe_records` bounds that declaration only by the file's
        // own length, and trailing padding raises that bound for free: measured,
        // a 4 195 160-byte file holding 80 samples produced two `Vec<f64>` of
        // 4 195 160 capacity — 67 MB from a 4 MB file, a 16× amplification. At
        // the wasm boundary's own 512 MiB file limit the same shape asks for
        // 8 GiB, which is twice the whole wasm32 address space and therefore a
        // trap rather than an error. Testing `records[i]` alone missed it,
        // because that is `min(declared, physical)` and the physical count of a
        // padded file is small.
        let declared = self.records[group_index];
        let claimed = group.raw_channel_group().block.cycles_nr;
        if declared.max(claimed) > MAX_RECORDS {
            return Err(MeasurementError::Parse(format!(
                "Channel group {group_index} claims {claimed} record(s) and holds {declared}, \
                 above the {MAX_RECORDS}-record limit this reader can hold in memory at once. \
                 Cut the recording to a shorter time range and re-export it."
            )));
        }

        let mut values = read_samples(group, &channels, target)?;
        let mut time = if target == master {
            // mdf4j's record factory claimed the master before it ever compared
            // names, so asking the Java for the time channel itself reported
            // "not found". Returning it against itself is the obvious answer.
            values.clone()
        } else {
            read_samples(group, &channels, master)?
        };

        // Equal lengths are a contract of `ChannelData`, and three numbers can
        // disagree: the group's declared cycle count, the records physically in
        // the data blocks, and — for a VLSD channel, whose samples live in a
        // separate `##SD` stream rather than in the record — the sample count of
        // that stream. The shortest is the only one all three agree on, so
        // truncate to it instead of padding an axis nobody measured.
        let n = values.len().min(time.len()).min(declared as usize);
        values.truncate(n);
        time.truncate(n);
        // `truncate` frees nothing, and the capacity these vectors carry is the
        // group's *declared* cycle count, not their length — so an
        // over-declaring file would leave up to `MAX_RECORDS` × 8 bytes retained
        // per channel behind a `len()` of five. That matters beyond tidiness:
        // the wasm boundary decides whether to keep a decoded channel in its
        // read cache from `data.len()`, and would otherwise hold two orders of
        // magnitude more than it accounted for. For a well-formed file capacity
        // already equals length and this is a no-op.
        values.shrink_to_fit();
        time.shrink_to_fit();

        Ok(ChannelData { time, values })
    }
}

fn group_label(index: usize, group: &ChannelGroup<'_>) -> String {
    group
        .name()
        .ok()
        .flatten()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("group {index}"))
}

fn no_such_group(asked: usize, groups: &[ChannelGroup<'_>]) -> MeasurementError {
    let listed: Vec<String> = groups
        .iter()
        .enumerate()
        .take(MAX_LISTED)
        .map(|(i, g)| format!("{i} (\"{}\")", group_label(i, g)))
        .collect();
    MeasurementError::NotFound(format!(
        "Channel group {asked} is not in this measurement. It has {} group(s): {}.",
        groups.len(),
        join_with_overflow(&listed, groups.len())
    ))
}

fn no_such_channel(group_index: usize, asked: &str, names: &[String]) -> MeasurementError {
    let listed: Vec<String> = names
        .iter()
        .take(MAX_LISTED)
        .map(|n| format!("\"{n}\""))
        .collect();
    MeasurementError::NotFound(format!(
        "Channel \"{asked}\" is not in channel group {group_index}. It has {} channel(s): {}.",
        names.len(),
        join_with_overflow(&listed, names.len())
    ))
}

fn join_with_overflow(listed: &[String], total: usize) -> String {
    let mut joined = listed.join(", ");
    if total > listed.len() {
        joined.push_str(&format!(", and {} more", total - listed.len()));
    }
    joined
}

/// Decode one channel of `group` as `f64`, invalid samples as `NaN`.
///
/// `mf4-rs`'s `values_as_f64` is already the Java's `AS_DOUBLE` visitor: every
/// numeric width widens to `f64`, and a sample the invalidation bits mark bad
/// becomes `NaN` rather than disappearing, so indices stay aligned with the time
/// master.
///
/// The one place it is *worse* than the Java is a channel carrying a
/// value-to-text conversion — a gear position, a state machine. mdf4j applied
/// identity and linear conversions only, so those channels came back as their
/// raw enum codes: unlabelled, but plottable. `mf4-rs` applies the table,
/// produces a `String`, and then cannot widen a string to `f64` — every sample
/// becomes `NaN`. Re-reading through a copy of the channel block with the
/// conversion detached restores the Java's numbers exactly. (`TextToValue` is
/// left alone: it converts *towards* numbers, so it is a gain, not a loss.)
fn read_samples(
    group: &ChannelGroup<'_>,
    channels: &[Channel<'_>],
    index: usize,
) -> Result<Vec<f64>> {
    let block = channels[index].block();

    if !produces_text(block) {
        return channels[index]
            .values_as_f64()
            .map_err(|e| MeasurementError::Parse(describe(&e)));
    }

    let mut bare = block.clone();
    bare.conversion = None;
    bare.conversion_addr = 0;
    let raw = RawChannel { block: bare };
    let stripped = Channel::new(
        &raw.block,
        group.raw_data_group(),
        group.raw_channel_group(),
        &raw,
        group.mmap(),
    );
    stripped
        .values_as_f64()
        .map_err(|e| MeasurementError::Parse(describe(&e)))
}

/// Does this channel's conversion turn numbers into text?
fn produces_text(block: &ChannelBlock) -> bool {
    matches!(
        block.conversion.as_ref().map(|c| c.cc_type),
        Some(
            ConversionType::ValueToText
                | ConversionType::RangeToText
                | ConversionType::TextToText
                | ConversionType::BitfieldText
        )
    )
}

/// Is this channel the group's time axis?
///
/// `cn_type` 2 is a stored master, 3 a virtual one; both are masters, and
/// [`MeasurementSource::channel`] is what refuses to *read* the virtual kind —
/// metadata should still report which channel is the axis. `cn_sync_type` 1 is
/// time; 2/3/4 are angle, distance and index masters, which are not a time axis
/// and are reported as ordinary channels. A master with sync type 0 ("none") is
/// out of spec but common enough in the wild, and time is the only thing it can
/// reasonably mean.
fn is_time_master(block: &ChannelBlock) -> bool {
    matches!(block.channel_type, 2 | 3) && matches!(block.sync_type, 0 | 1)
}

/// Classify a channel for the frontend's rendering choice.
///
/// The Java's `kindOf` only ever answered "analog" or "string" — mdf4j's type
/// visitor had no boolean case, and neither did the Python sidecar (`_kind`:
/// `i`/`u`/`f` → analog, else string). This adds the third:
///
/// * a **1-bit integer** is a boolean by construction;
/// * an integer whose *declared* raw domain is exactly 0..1 is a boolean, but
///   only if no conversion rescales it — `cn_val_range` describes the raw value,
///   not the physical one.
///
/// Both tests read block headers only. Deciding "the samples happen to be all 0s
/// and 1s" would mean decoding every record of every channel during
/// `metadata()`, which is precisely the work the Java's `parseMetadata` was
/// careful never to do; in practice the writers that mean "boolean" say so.
/// asammdf, for one, leaves `cn_val_range` unset, so its `uint8` 0/1 channels
/// land on Analog here exactly as they did in Java.
fn kind_of(block: &ChannelBlock) -> ChannelKind {
    match block.data_type {
        DataType::UnsignedIntegerLE
        | DataType::UnsignedIntegerBE
        | DataType::SignedIntegerLE
        | DataType::SignedIntegerBE => {
            if block.bit_count == 1 || declares_zero_one_domain(block) {
                ChannelKind::Boolean
            } else {
                ChannelKind::Analog
            }
        }
        DataType::FloatLE | DataType::FloatBE => ChannelKind::Analog,
        // Strings, byte arrays, MIME payloads, CANopen date/time and complex
        // values all land here. The Java's `visitElse` did the same: anything
        // that is not a plain number renders as text.
        _ => ChannelKind::Text,
    }
}

/// `cn_flags` bit 3 — "value range valid" — with a raw domain of exactly 0..1.
fn declares_zero_one_domain(block: &ChannelBlock) -> bool {
    const CN_FLAG_VAL_RANGE_VALID: u32 = 0x08;
    block.flags & CN_FLAG_VAL_RANGE_VALID != 0
        && block.min_raw_value == 0.0
        && block.max_raw_value == 1.0
        && matches!(
            block.conversion.as_ref().map(|c| c.cc_type),
            None | Some(ConversionType::Identity)
        )
}

/// Establish how many records a channel group really has, and reject the storage
/// forms this reader cannot decode.
///
/// Walking the group's data blocks is the probe for two of the three Java
/// limitations at once: `mf4-rs` refuses a `##DZ` block by id, and refuses a data
/// group that interleaves several channel groups behind a record id ("sorted
/// files only"). Doing it at [`open`] means both are reported once, at load,
/// instead of on every read.
///
/// The count itself is `min(cn_cycle_count, records physically present)`, because
/// either number alone can be wrong: a truncated file over-declares its cycle
/// count, and a `##DT` block padded to 8-byte alignment over-supplies records
/// whenever a record is shorter than the padding. A declared count of zero is
/// treated as "not declared" and the physical count wins.
///
/// It is also the guard against an abort. `Channel::values{,_as_f64}` open with
/// `Vec::with_capacity(cn_cycle_count as usize)` — a `u64` straight out of the
/// file. Flip four bytes in a `##CG` block and `mf4-rs` asks the allocator for
/// terabytes; measured, a corrupt count produced an 8.7 TB request and the
/// process died. A record cannot be smaller than the bytes it occupies, so the
/// file's own length bounds the count, and anything above that bound is a lie.
fn probe_records(group: &ChannelGroup<'_>, index: usize, file_len: u64) -> Result<u64> {
    let cg = &group.raw_channel_group().block;
    let dg = &group.raw_data_group().block;

    let record_size = u64::from(dg.record_id_len)
        + u64::from(cg.samples_byte_nr)
        + u64::from(cg.invalidation_bytes_nr);

    let declared = cg.cycles_nr;
    let ceiling = file_len.checked_div(record_size).unwrap_or(0);
    if declared > ceiling {
        return Err(MeasurementError::Parse(format!(
            "Channel group {index} claims {declared} records of {record_size} byte(s) each, \
             which cannot fit in a {file_len}-byte file. The file is truncated or corrupt; \
             re-export the recording."
        )));
    }

    // `mf4-rs` refuses unsorted data groups too, but by returning a message we
    // would have to string-match. The condition is two public fields, so test it
    // here and say what it means for the user.
    if dg.record_id_len > 0 && group.raw_data_group().channel_groups.len() > 1 {
        return Err(MeasurementError::Parse(format!(
            "Channel group {index} is stored in an unsorted data group (several channel groups \
             interleaved in one record stream). This reader, like the frees server before it, \
             reads sorted files only — re-export the recording sorted."
        )));
    }

    let blocks = group
        .raw_data_group()
        .data_blocks(group.mmap())
        .map_err(|e| storage_error(&e, index))?;

    if record_size == 0 {
        return Ok(0);
    }
    let physical: u64 = blocks
        .iter()
        .map(|b| b.data.len() as u64 / record_size)
        .sum();

    Ok(if declared == 0 {
        physical
    } else {
        declared.min(physical)
    })
}

/// Turn a data-block walk failure into a sentence naming the storage form.
fn storage_error(err: &MdfError, index: usize) -> MeasurementError {
    if let MdfError::BlockIDError { actual, .. } = err {
        if actual == "##DZ" {
            return MeasurementError::Parse(format!(
                "Channel group {index} stores its samples in compressed (##DZ) data blocks. \
                 This reader has no decompressor — re-export the recording uncompressed."
            ));
        }
    }
    MeasurementError::Parse(format!(
        "Channel group {index} has unreadable sample storage: {}",
        describe(err)
    ))
}

// ---------------------------------------------------------------------------
// Pre-flight: bound the block graph before `mf4-rs` walks it
// ---------------------------------------------------------------------------

/// Reject files that would make `mf4-rs` abort the process while parsing.
///
/// `MDF::from_bytes` resolves every channel's `##CC` conversion block eagerly,
/// and `ConversionBlock::from_bytes` trusts that block's own `links_nr`: it
/// sizes a `Vec` from it (`capacity overflow` → panic) and then indexes past the
/// link section without a bounds check. One flipped byte in a `cc_conversion`
/// link reaches both; fuzzing single-byte mutations of a writer-produced file
/// hit it in well under a second.
///
/// It cannot be caught after the fact: the wasm profile sets `panic = "abort"`,
/// so `catch_unwind` does not exist on the target that ships. The only place to
/// stand is in front. So walk the same `##HD → ##DG → ##CG → ##CN → ##CC` chain
/// first, with nothing but bounds-checked reads, and refuse any block whose
/// header contradicts itself.
///
/// Every test below is an invariant of the format ("a block's link section fits
/// inside the block; a block fits inside the file"), never a heuristic, so a
/// well-formed recording cannot be refused by it. The offsets are those of the
/// MDF 4.1 block layouts, which are frozen.
fn validate_block_graph(bytes: &[u8]) -> Result<()> {
    // The identification block is 64 bytes and the file header follows it.
    const HD_ADDR: u64 = 64;
    const HD_MIN: u64 = 104;
    const DG_MIN: u64 = 64;
    const CG_MIN: u64 = 104;
    const CN_MIN: u64 = 160;

    if bytes.len() < 168 {
        return Err(MeasurementError::Parse(format!(
            "Not a readable MDF4 file: it is {} byte(s) long, and an MDF4 file needs at least \
             168 for its identification and header blocks.",
            bytes.len()
        )));
    }

    // The 8-byte file identifier, checked here rather than left to `mf4-rs`, so
    // that "you dropped in the wrong file" — much the likeliest failure — is the
    // first thing the user is told, instead of a complaint about the block at
    // byte 64. The file's own first bytes come back with it.
    if &bytes[0..8] != b"MDF     " {
        return Err(MeasurementError::Parse(format!(
            "Not an MDF4 file: it should begin with the identifier \"MDF     \", but begins with \
             {:?}.",
            String::from_utf8_lossy(&bytes[0..8])
        )));
    }

    let mut budget = MAX_BLOCKS;
    check_header(bytes, HD_ADDR, b"##HD", "file header", HD_MIN, &mut budget)?;

    let mut dg_addr = link_at(bytes, HD_ADDR, 24);
    let mut seen_dg = std::collections::HashSet::new();
    while dg_addr != 0 {
        if !visit(&mut seen_dg, dg_addr) {
            return Err(link_cycle("data group", dg_addr));
        }
        check_header(bytes, dg_addr, b"##DG", "data group", DG_MIN, &mut budget)?;
        let data_addr = link_at(bytes, dg_addr, 40);

        let mut cg_addr = link_at(bytes, dg_addr, 32);
        let mut seen_cg = std::collections::HashSet::new();
        while cg_addr != 0 {
            if !visit(&mut seen_cg, cg_addr) {
                return Err(link_cycle("channel group", cg_addr));
            }
            check_header(
                bytes,
                cg_addr,
                b"##CG",
                "channel group",
                CG_MIN,
                &mut budget,
            )?;
            // Once per *channel group*, because that is how often
            // `probe_records` walks it. See [`check_storage_chain`].
            check_storage_chain(bytes, data_addr, &mut budget)?;

            let mut cn_addr = link_at(bytes, cg_addr, 32);
            let mut seen_cn = std::collections::HashSet::new();
            while cn_addr != 0 {
                if !visit(&mut seen_cn, cn_addr) {
                    return Err(link_cycle("channel", cn_addr));
                }
                check_header(bytes, cn_addr, b"##CN", "channel", CN_MIN, &mut budget)?;

                let cc_addr = link_at(bytes, cn_addr, 56);
                if cc_addr != 0 {
                    check_conversion(bytes, cc_addr, &mut budget)?;
                }
                cn_addr = link_at(bytes, cn_addr, 24);
            }
            cg_addr = link_at(bytes, cg_addr, 24);
        }
        dg_addr = link_at(bytes, dg_addr, 24);
    }

    Ok(())
}

/// Record `addr` as visited; `false` if the chain has looped.
///
/// A hash set rather than a `Vec` scan, and the difference is not a
/// micro-optimisation. [`MAX_BLOCKS`] bounds how many blocks the walk *visits*,
/// but a linear membership test makes the walk cost the **square** of that, and
/// the block layout lets a crafted file reach the ceiling cheaply: `##DG`
/// blocks may overlap, and every field this walk reads fits in the first 40
/// bytes, so a chain of a million of them needs only 40 MB — well under the
/// 512 MiB the wasm boundary will hold. Measured on the `Vec` form, in release:
/// 10 000 blocks took 0.11 s, 40 000 took 1.0 s and 160 000 took **17.6 s**,
/// which extrapolates to roughly eleven minutes at the ceiling — a worker
/// wedged with no way to cancel it, from a file the user merely dropped in.
fn visit(seen: &mut std::collections::HashSet<u64>, addr: u64) -> bool {
    seen.insert(addr)
}

fn link_cycle(what: &str, addr: u64) -> MeasurementError {
    MeasurementError::Parse(format!(
        "The MDF4 file's {what} list loops back on itself at byte {addr}; its block links are \
         corrupt. Re-export the recording."
    ))
}

/// Spend `n` of the shared block budget, or refuse.
///
/// One budget across the whole pre-flight, not one per chain: a file that is
/// hostile is hostile in aggregate, and the *only* number that bounds the work
/// this reader does is the total number of block visits.
fn charge(budget: &mut usize, n: usize) -> Result<()> {
    match budget.checked_sub(n) {
        Some(left) => {
            *budget = left;
            Ok(())
        }
        None => Err(MeasurementError::Parse(format!(
            "The MDF4 file links more than {MAX_BLOCKS} blocks. That is not a recording; its \
             block links are corrupt."
        ))),
    }
}

/// Walk one channel group's sample-storage chain, charging the shared budget.
///
/// This exists because [`probe_records`] calls `mf4-rs`'s `data_blocks` **once
/// per channel group**, and that walk is `O(chain length)` with no budget of
/// its own. Nothing forces a data group's storage to be its own: a thousand
/// data groups may all point at one `##DL` chain, or at a thousand different
/// offsets into one overlapping run of them. Either way the reader does
/// `groups × chain` work for a file whose *size* is only `40 × chain`.
///
/// Measured before this guard, in release: 10 groups over a 20 000-block chain
/// took 0.10 s, 40 groups 0.23 s, and 40 groups over an 80 000-block chain
/// 0.95 s — linear in the product, as expected. A 40 MB file (a thousand
/// groups over a million-block chain) is about **five minutes**, and the
/// boundary's 512 MiB limit buys hours. `open` is the call the user makes by
/// dropping a file on the page, so that is the worker wedged with no cancel.
///
/// Charging here, against the same [`MAX_BLOCKS`], bounds the pre-flight and
/// the real walk together: the two do the same work, so a file that would make
/// `probe_records` expensive cannot get past this. Fragment links are charged
/// too — `mf4-rs` parses one data block per link, also once per channel group.
///
/// The chain ends at the first block that is not a `##DL`. Whether a `##DT`,
/// `##DV` or `##DZ` is *readable* is [`probe_records`]'s judgement, not this
/// walk's; this only bounds how many blocks the judgement has to look at.
fn check_storage_chain(bytes: &[u8], head: u64, budget: &mut usize) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    let mut addr = head;
    while addr != 0 {
        if !visit(&mut seen, addr) {
            return Err(link_cycle("data block", addr));
        }
        let start =
            usize::try_from(addr).map_err(|_| out_of_file("data block", addr, bytes.len()))?;
        let Some(id) = bytes.get(start..start.saturating_add(4)) else {
            return Err(out_of_file("data block", addr, bytes.len()));
        };
        if id != b"##DL" {
            return charge(budget, 1);
        }
        // A DLBLOCK is a 24-byte header, the `next` link, at least one data
        // link, and the flags/count tail.
        let (_, links_nr) = check_header(bytes, addr, b"##DL", "data list", 40, budget)?;
        charge(budget, usize::try_from(links_nr).unwrap_or(usize::MAX))?;
        addr = link_at(bytes, addr, 24);
    }
    Ok(())
}

/// Read a block header, checking that the block is where it says it is and that
/// its declared link section fits inside it.
///
/// Returns `(block_len, links_nr)`.
fn check_header(
    bytes: &[u8],
    addr: u64,
    expect: &[u8; 4],
    what: &str,
    min_len: u64,
    budget: &mut usize,
) -> Result<(u64, u64)> {
    charge(budget, 1)?;

    let start = usize::try_from(addr).map_err(|_| out_of_file(what, addr, bytes.len()))?;
    let head_end = start
        .checked_add(24)
        .ok_or_else(|| out_of_file(what, addr, bytes.len()))?;
    if head_end > bytes.len() {
        return Err(out_of_file(what, addr, bytes.len()));
    }

    let id = &bytes[start..start + 4];
    if id != expect {
        return Err(MeasurementError::Parse(format!(
            "The MDF4 file's block links are corrupt: byte {addr} should begin the {what} block \
             \"{}\", but holds {:?}.",
            String::from_utf8_lossy(expect),
            String::from_utf8_lossy(id)
        )));
    }

    let block_len = u64::from_le_bytes(bytes[start + 8..start + 16].try_into().unwrap());
    let links_nr = u64::from_le_bytes(bytes[start + 16..start + 24].try_into().unwrap());

    if block_len < min_len {
        return Err(MeasurementError::Parse(format!(
            "The MDF4 file's {what} block at byte {addr} declares a length of {block_len} \
             byte(s); an MDF 4.1 {what} block is at least {min_len}."
        )));
    }
    if addr.saturating_add(block_len) > bytes.len() as u64 {
        return Err(out_of_file(what, addr, bytes.len()));
    }
    // The link section is `links_nr` 8-byte addresses after the 24-byte header.
    if 24u64.saturating_add(links_nr.saturating_mul(8)) > block_len {
        return Err(MeasurementError::Parse(format!(
            "The MDF4 file's {what} block at byte {addr} declares {links_nr} link(s) but is only \
             {block_len} byte(s) long. The file is corrupt; re-export the recording."
        )));
    }

    Ok((block_len, links_nr))
}

/// The `##CC` check that closes the abort.
fn check_conversion(bytes: &[u8], addr: u64, budget: &mut usize) -> Result<()> {
    let (block_len, links_nr) = check_header(bytes, addr, b"##CC", "conversion", 24, budget)?;

    // A CCBLOCK always carries its four fixed links (name, unit, comment,
    // inverse) before any `cc_ref`, and `mf4-rs` reads all four unconditionally.
    if links_nr < 4 {
        return Err(MeasurementError::Parse(format!(
            "The MDF4 file's conversion block at byte {addr} declares {links_nr} link(s); an MDF \
             4.1 conversion block always has at least 4."
        )));
    }
    // After the links come cc_type, cc_precision, cc_flags, cc_ref_count and
    // cc_val_count — eight bytes `mf4-rs` reads without a bounds check.
    let after_links = 24u64.saturating_add(links_nr.saturating_mul(8));
    if after_links.saturating_add(8) > block_len {
        return Err(MeasurementError::Parse(format!(
            "The MDF4 file's conversion block at byte {addr} is {block_len} byte(s) long, too \
             short for the {links_nr} link(s) it declares. The file is corrupt; re-export the \
             recording."
        )));
    }
    Ok(())
}

fn out_of_file(what: &str, addr: u64, file_len: usize) -> MeasurementError {
    MeasurementError::Parse(format!(
        "The MDF4 file's {what} block is at byte {addr}, past the end of the {file_len}-byte \
         file. The file is truncated or its block links are corrupt."
    ))
}

/// Read the link `offset` bytes into the block at `addr`.
///
/// Only ever called after [`check_header`] has established that the block — and
/// therefore this link, which lies inside its minimum length — is within the
/// file. The `0` fallback exists so that a reordering could never index out of
/// bounds; it reads as "no link", which ends the walk.
fn link_at(bytes: &[u8], addr: u64, offset: u64) -> u64 {
    let Ok(start) = usize::try_from(addr.saturating_add(offset)) else {
        return 0;
    };
    match bytes.get(start..start.saturating_add(8)) {
        Some(slice) => u64::from_le_bytes(slice.try_into().unwrap()),
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Render an [`MdfError`] as a sentence fit for a user.
///
/// `MdfError`'s own `Display` is not: `TooShortBuffer` interpolates `file!()`
/// and `line!()`, so passing it through would print the absolute path of the
/// build machine's cargo registry into a browser error message. Every variant is
/// spelled out here instead, and the match is deliberately exhaustive — if a
/// future `mf4-rs` adds a variant this stops compiling and somebody writes the
/// sentence, rather than a registry path leaking out again.
fn describe(err: &MdfError) -> String {
    match err {
        MdfError::TooShortBuffer {
            actual, expected, ..
        } => format!(
            "The MDF4 file is truncated or its block links are corrupt: a block needs {expected} \
             byte(s) but the file is {actual}."
        ),
        MdfError::FileIdentifierError(found) => format!(
            "Not an MDF4 file: it should begin with the identifier \"MDF     \", but begins with \
             {found:?}."
        ),
        MdfError::FileVersioningError(version) => {
            let pretty = version
                .parse::<u16>()
                .map(|v| format!("{}.{:02}", v / 100, v % 100))
                .unwrap_or_else(|_| version.clone());
            format!(
                "This file is MDF {pretty}. This reader needs MDF 4.10 or newer — re-export the \
                 recording as MDF 4."
            )
        }
        MdfError::BlockIDError { actual, expected } => format!(
            "The MDF4 file's block links are corrupt: expected a {expected} block, found \
             {actual:?}."
        ),
        MdfError::IOError(e) => format!("The MDF4 file could not be read: {e}."),
        MdfError::InvalidVersionString(detail) => {
            format!("The MDF4 file's header does not carry a readable version number ({detail}).")
        }
        MdfError::BlockLinkError(detail) => {
            format!("The MDF4 file's block links are corrupt: {detail}.")
        }
        MdfError::BlockSerializationError(detail) => {
            format!("The MDF4 file uses a structure this reader cannot handle: {detail}.")
        }
        MdfError::ConversionChainTooDeep { max_depth } => format!(
            "A channel in this file chains more than {max_depth} unit conversions, which this \
             reader will not follow."
        ),
        MdfError::ConversionChainCycle { address } => format!(
            "A channel's unit conversion at byte {address} refers back to itself; the file is \
             corrupt."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mf4_rs::blocks::common::BlockHeader;
    use mf4_rs::blocks::text_block::TextBlock;
    use mf4_rs::parsing::decoder::DecodedValue;
    use mf4_rs::writer::MdfWriter;
    use std::cell::RefCell;
    use std::io::{Seek, SeekFrom, Write};
    use std::rc::Rc;

    // -----------------------------------------------------------------
    // Fixtures. There is no `.mf4` in this repo and no filesystem on the
    // target, so every fixture is written by `mf4-rs`'s own writer into a
    // `Vec<u8>` and read straight back — a real round trip, and one that runs
    // identically under wasm.
    // -----------------------------------------------------------------

    /// A `Write + Seek` sink that hands the finished bytes back.
    ///
    /// `MdfWriter::finalize` consumes the writer and boxes the sink, so the
    /// buffer has to be shared rather than owned.
    #[derive(Clone, Default)]
    struct SharedSink {
        buf: Rc<RefCell<Vec<u8>>>,
        pos: Rc<RefCell<u64>>,
    }

    impl Write for SharedSink {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            let mut buf = self.buf.borrow_mut();
            let mut pos = self.pos.borrow_mut();
            let start = *pos as usize;
            if buf.len() < start + data.len() {
                buf.resize(start + data.len(), 0);
            }
            buf[start..start + data.len()].copy_from_slice(data);
            *pos += data.len() as u64;
            Ok(data.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Seek for SharedSink {
        fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
            let len = self.buf.borrow().len() as i64;
            let cur = *self.pos.borrow() as i64;
            let next = match from {
                SeekFrom::Start(x) => x as i64,
                SeekFrom::End(x) => len + x,
                SeekFrom::Current(x) => cur + x,
            };
            if next < 0 {
                return Err(std::io::Error::other("seek before start"));
            }
            *self.pos.borrow_mut() = next as u64;
            Ok(next as u64)
        }
    }

    /// A hand-built linear CCBLOCK: `phys = a + b · raw`.
    ///
    /// The writer only ships a value-to-text helper, and spelling the bytes out
    /// is also the clearest statement of what the reader is expected to find:
    /// 24-byte header, four fixed links, the five type/count fields, `cc_val`.
    fn linear_conversion(a: f64, b: f64) -> Vec<u8> {
        let header = BlockHeader {
            id: "##CC".to_string(),
            reserved0: 0,
            block_len: 24 + 4 * 8 + 8 + 2 * 8,
            links_nr: 4,
        };
        let mut out = header.to_bytes().unwrap();
        // cc_tx_name, cc_md_unit, cc_md_comment, cc_cc_inverse — all absent.
        out.extend_from_slice(&[0u8; 32]);
        out.push(1); // cc_type = Linear
        out.push(0); // cc_precision
        out.extend_from_slice(&0u16.to_le_bytes()); // cc_flags
        out.extend_from_slice(&0u16.to_le_bytes()); // cc_ref_count
        out.extend_from_slice(&2u16.to_le_bytes()); // cc_val_count
        out.extend_from_slice(&a.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
        out
    }

    /// Two groups exercising every metadata answer this reader can give.
    ///
    /// Group 0 "engine": `time` (master, s) · `speed` (f64, m/s) · `gear` (u8
    /// with a value-to-text conversion) · `brake` (1-bit) · `tag` (4-byte
    /// Latin-1). Group 1 "coolant": `t` (master) · `temp_raw` (u16, linear
    /// `0.1·raw − 40`).
    fn two_group_file() -> Vec<u8> {
        let sink = SharedSink::default();
        let bytes = sink.buf.clone();
        let mut w = MdfWriter::new_from_writer(sink);
        w.init_mdf_file().unwrap();

        // Units are TX blocks the writer does not create itself, so write them
        // first and point the channels at them.
        let unit_s = w
            .write_block_with_id(&TextBlock::new("s").to_bytes().unwrap(), "u_s")
            .unwrap();
        let unit_mps = w
            .write_block_with_id(&TextBlock::new("m/s").to_bytes().unwrap(), "u_mps")
            .unwrap();
        let unit_degc = w
            .write_block_with_id(&TextBlock::new("degC").to_bytes().unwrap(), "u_degc")
            .unwrap();

        let g0 = w.add_channel_group(None, |_| {}).unwrap();
        w.set_channel_group_name(&g0, "engine").unwrap();
        let t0 = w
            .add_channel(&g0, None, |c| {
                c.data_type = DataType::FloatLE;
                c.bit_count = 64;
                c.name = Some("time".into());
                c.unit_addr = unit_s;
            })
            .unwrap();
        w.set_time_channel(&t0).unwrap();
        let speed = w
            .add_channel(&g0, Some(&t0), |c| {
                c.data_type = DataType::FloatLE;
                c.bit_count = 64;
                c.name = Some("speed".into());
                c.unit_addr = unit_mps;
            })
            .unwrap();
        let gear = w
            .add_channel(&g0, Some(&speed), |c| {
                c.data_type = DataType::UnsignedIntegerLE;
                c.bit_count = 8;
                c.name = Some("gear".into());
            })
            .unwrap();
        let brake = w
            .add_channel(&g0, Some(&gear), |c| {
                c.data_type = DataType::UnsignedIntegerLE;
                c.bit_count = 1;
                c.name = Some("brake".into());
            })
            .unwrap();
        w.add_channel(&g0, Some(&brake), |c| {
            // A byte array rather than a fixed-length string only because the
            // writer refuses to encode the latter; both land on `Text`.
            c.data_type = DataType::ByteArray;
            c.bit_count = 32;
            c.name = Some("tag".into());
        })
        .unwrap();
        w.add_value_to_text_conversion(&[(0i64, "P"), (1, "R"), (2, "N")], "?", Some(&gear))
            .unwrap();

        w.start_data_block_for_cg(&g0, 0).unwrap();
        for i in 0..10u32 {
            w.write_record(
                &g0,
                &[
                    DecodedValue::Float(f64::from(i) * 0.5),
                    DecodedValue::Float(100.0 + f64::from(i)),
                    DecodedValue::UnsignedInteger(u64::from(i % 3)),
                    DecodedValue::UnsignedInteger(u64::from(i % 2)),
                    DecodedValue::ByteArray(b"abcd".to_vec()),
                ],
            )
            .unwrap();
        }
        w.finish_data_block(&g0).unwrap();

        let cc = w
            .write_block_with_id(&linear_conversion(-40.0, 0.1), "cc_lin")
            .unwrap();
        let g1 = w.add_channel_group(None, |_| {}).unwrap();
        w.set_channel_group_name(&g1, "coolant").unwrap();
        let t1 = w
            .add_channel(&g1, None, |c| {
                c.data_type = DataType::FloatLE;
                c.bit_count = 64;
                c.name = Some("t".into());
                c.unit_addr = unit_s;
            })
            .unwrap();
        w.set_time_channel(&t1).unwrap();
        w.add_channel(&g1, Some(&t1), |c| {
            c.data_type = DataType::UnsignedIntegerLE;
            c.bit_count = 16;
            c.name = Some("temp_raw".into());
            c.unit_addr = unit_degc;
            c.conversion_addr = cc;
        })
        .unwrap();

        w.start_data_block_for_cg(&g1, 0).unwrap();
        for i in 0..4u32 {
            w.write_record(
                &g1,
                &[
                    DecodedValue::Float(f64::from(i)),
                    DecodedValue::UnsignedInteger(u64::from(i) * 100),
                ],
            )
            .unwrap();
        }
        w.finish_data_block(&g1).unwrap();
        w.finalize().unwrap();

        let out = bytes.borrow().clone();
        out
    }

    /// One group, one master, one channel — the smallest thing worth patching.
    fn minimal_file() -> Vec<u8> {
        let sink = SharedSink::default();
        let bytes = sink.buf.clone();
        let mut w = MdfWriter::new_from_writer(sink);
        w.init_mdf_file().unwrap();
        let cg = w.add_channel_group(None, |_| {}).unwrap();
        w.set_channel_group_name(&cg, "solo").unwrap();
        let t = w
            .add_channel(&cg, None, |c| {
                c.data_type = DataType::FloatLE;
                c.bit_count = 64;
                c.name = Some("time".into());
            })
            .unwrap();
        w.set_time_channel(&t).unwrap();
        w.add_channel(&cg, Some(&t), |c| {
            c.data_type = DataType::FloatLE;
            c.bit_count = 64;
            c.name = Some("x".into());
        })
        .unwrap();
        w.start_data_block_for_cg(&cg, 0).unwrap();
        for i in 0..5u32 {
            w.write_record(
                &cg,
                &[
                    DecodedValue::Float(f64::from(i)),
                    DecodedValue::Float(f64::from(i) * 2.0),
                ],
            )
            .unwrap();
        }
        w.finish_data_block(&cg).unwrap();
        w.finalize().unwrap();
        let out = bytes.borrow().clone();
        out
    }

    /// Offset of the first occurrence of `marker`.
    fn find_block(bytes: &[u8], marker: &[u8; 4]) -> usize {
        bytes
            .windows(4)
            .position(|w| w == marker)
            .unwrap_or_else(|| panic!("no {} block", String::from_utf8_lossy(marker)))
    }

    /// Offset of the sole occurrence of `marker`, asserting there is only one.
    fn sole_block(bytes: &[u8], marker: &[u8; 4]) -> usize {
        let hits: Vec<usize> = bytes
            .windows(4)
            .enumerate()
            .filter(|(_, w)| *w == marker)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one {} block, found {}",
            String::from_utf8_lossy(marker),
            hits.len()
        );
        hits[0]
    }

    fn parse_message(err: &MeasurementError) -> String {
        match err {
            MeasurementError::Parse(m) => m.clone(),
            other => panic!("expected a Parse error, got {other:?}"),
        }
    }

    fn notfound_message(err: &MeasurementError) -> String {
        match err {
            MeasurementError::NotFound(m) => m.clone(),
            other => panic!("expected a NotFound error, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // Positive path
    // -----------------------------------------------------------------

    #[test]
    fn metadata_describes_both_groups() {
        let source = open(two_group_file()).unwrap();
        let meta = source.metadata().unwrap();
        assert_eq!(meta.groups.len(), 2);

        let g0 = &meta.groups[0];
        assert_eq!(g0.index, 0);
        assert_eq!(g0.name, "engine");
        assert_eq!(g0.records, 10);
        let names: Vec<&str> = g0.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["time", "speed", "gear", "brake", "tag"]);
        assert_eq!(g0.channels[0].unit.as_deref(), Some("s"));
        assert_eq!(g0.channels[1].unit.as_deref(), Some("m/s"));
        assert!(g0.channels[0].time_master);
        assert!(g0.channels.iter().skip(1).all(|c| !c.time_master));

        let g1 = &meta.groups[1];
        assert_eq!(g1.index, 1);
        assert_eq!(g1.name, "coolant");
        assert_eq!(g1.records, 4);
        assert_eq!(g1.channels[1].name, "temp_raw");
        assert_eq!(g1.channels[1].unit.as_deref(), Some("degC"));
    }

    #[test]
    fn kinds_follow_the_declared_type() {
        let source = open(two_group_file()).unwrap();
        let meta = source.metadata().unwrap();
        let g0 = &meta.groups[0];
        assert_eq!(g0.channels[0].kind, ChannelKind::Analog); // time, f64
        assert_eq!(g0.channels[1].kind, ChannelKind::Analog); // speed, f64
        assert_eq!(g0.channels[2].kind, ChannelKind::Analog); // gear, 8-bit uint
        assert_eq!(g0.channels[3].kind, ChannelKind::Boolean); // brake, 1-bit
        assert_eq!(g0.channels[4].kind, ChannelKind::Text); // tag, Latin-1
    }

    #[test]
    fn a_declared_zero_one_domain_is_boolean() {
        // `cn_val_range_valid` (cn_flags bit 3) with raw min 0 / max 1. asammdf
        // leaves the flag clear, so this is the only way to reach the branch.
        let mut block = ChannelBlock {
            data_type: DataType::UnsignedIntegerLE,
            bit_count: 8,
            ..ChannelBlock::default()
        };
        assert_eq!(kind_of(&block), ChannelKind::Analog);

        block.flags = 0x08;
        block.min_raw_value = 0.0;
        block.max_raw_value = 1.0;
        assert_eq!(kind_of(&block), ChannelKind::Boolean);

        // A declared range of 0..255 is just an unsigned byte.
        block.max_raw_value = 255.0;
        assert_eq!(kind_of(&block), ChannelKind::Analog);
    }

    #[test]
    fn channel_pairs_samples_with_the_time_master() {
        let source = open(two_group_file()).unwrap();
        let data = source.channel(0, "speed").unwrap();
        assert_eq!(data.len(), 10);
        assert_eq!(data.time.len(), data.values.len());
        assert!(!data.is_empty());
        assert_eq!(data.time[0], 0.0);
        assert_eq!(data.time[9], 4.5);
        assert_eq!(data.values[0], 100.0);
        assert_eq!(data.values[9], 109.0);
    }

    #[test]
    fn a_linear_conversion_is_applied() {
        // phys = −40 + 0.1 · raw, over raw = 0, 100, 200, 300.
        let source = open(two_group_file()).unwrap();
        let data = source.channel(1, "temp_raw").unwrap();
        assert_eq!(data.values, vec![-40.0, -30.0, -20.0, -10.0]);
        assert_eq!(data.time, vec![0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_value_to_text_channel_reads_as_raw_numbers() {
        // The Java's documented behaviour: mdf4j applied identity and linear
        // conversions only, so an enum channel came back as its codes. mf4-rs
        // resolves the table to "P"/"R"/"N" and then hands back all-NaN.
        let source = open(two_group_file()).unwrap();
        let data = source.channel(0, "gear").unwrap();
        assert_eq!(
            data.values,
            vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0]
        );
    }

    #[test]
    fn an_unreadable_sample_is_nan_not_a_missing_row() {
        // A Latin-1 channel has no f64 widening, so every sample is NaN — but
        // the row survives, because dropping it would slide every later sample
        // onto the wrong timestamp.
        let source = open(two_group_file()).unwrap();
        let data = source.channel(0, "tag").unwrap();
        assert_eq!(data.len(), 10);
        assert_eq!(data.time.len(), data.values.len());
        assert!(data.values.iter().all(|v| v.is_nan()));
        assert_eq!(data.time[3], 1.5);
    }

    #[test]
    fn the_master_can_be_read_against_itself() {
        // mdf4j's record factory claimed the master before comparing names, so
        // the Java answered "channel not found" for the time channel.
        let source = open(two_group_file()).unwrap();
        let data = source.channel(0, "time").unwrap();
        assert_eq!(data.time, data.values);
        assert_eq!(data.time.len(), 10);
    }

    #[test]
    fn an_exact_name_wins_over_a_case_insensitive_one() {
        let source = open(two_group_file()).unwrap();
        assert_eq!(source.channel(0, "SPEED").unwrap().values[0], 100.0);
        assert_eq!(source.channel(0, "speed").unwrap().values[0], 100.0);
    }

    // -----------------------------------------------------------------
    // Diagnostics
    // -----------------------------------------------------------------

    #[test]
    fn an_unknown_group_names_what_is_available() {
        let source = open(two_group_file()).unwrap();
        let err = source.channel(7, "speed").unwrap_err();
        let msg = notfound_message(&err);
        assert!(msg.contains('7'), "{msg}");
        assert!(msg.contains("engine"), "{msg}");
        assert!(msg.contains("coolant"), "{msg}");
        assert_eq!(err.code(), "CHANNEL_NOT_FOUND");
    }

    #[test]
    fn an_unknown_channel_quotes_the_name_and_lists_the_group() {
        let source = open(two_group_file()).unwrap();
        let msg = notfound_message(&source.channel(0, "rpm").unwrap_err());
        assert!(msg.contains("\"rpm\""), "{msg}");
        assert!(msg.contains("\"speed\""), "{msg}");
        assert!(msg.contains("\"gear\""), "{msg}");
    }

    #[test]
    fn a_group_without_a_time_master_is_an_error() {
        // Demote the master back to an ordinary channel (cn_type at CN+88).
        let mut bytes = minimal_file();
        let cn = find_block(&bytes, b"##CN");
        bytes[cn + 88] = 0;
        let source = open(bytes).unwrap();
        // Metadata still works — only the read needs an axis.
        assert!(source.metadata().unwrap().groups[0]
            .channels
            .iter()
            .all(|c| !c.time_master));
        let msg = parse_message(&source.channel(0, "x").unwrap_err());
        assert!(msg.contains("no time master"), "{msg}");
        assert!(msg.contains("solo"), "{msg}");
    }

    #[test]
    fn a_virtual_master_is_refused_rather_than_guessed() {
        let mut bytes = minimal_file();
        let cn = find_block(&bytes, b"##CN");
        bytes[cn + 88] = 3; // cn_type = virtual master
        let source = open(bytes).unwrap();
        // It is still reported as the axis...
        assert!(source.metadata().unwrap().groups[0].channels[0].time_master);
        // ...but reading refuses instead of returning an all-NaN time column.
        let msg = parse_message(&source.channel(0, "x").unwrap_err());
        assert!(msg.contains("virtual master"), "{msg}");
        assert!(msg.contains("\"time\""), "{msg}");
    }

    #[test]
    fn a_compressed_data_block_names_the_remedy() {
        let mut bytes = minimal_file();
        let dt = sole_block(&bytes, b"##DT");
        bytes[dt..dt + 4].copy_from_slice(b"##DZ");
        let msg = parse_message(&open(bytes).unwrap_err());
        assert!(msg.contains("##DZ"), "{msg}");
        assert!(msg.contains("re-export"), "{msg}");
    }

    #[test]
    fn an_unsorted_data_group_is_refused() {
        // Two channel groups in one data group, addressed by record id — the
        // Java's "sorted files only" limitation.
        let sink = SharedSink::default();
        let bytes = sink.buf.clone();
        let mut w = MdfWriter::new_from_writer(sink);
        w.init_mdf_file().unwrap();
        let dg = w.add_data_group(None).unwrap();
        let a = w.add_channel_group_with_dg(&dg, None, |_| {}).unwrap();
        let b = w.add_channel_group_with_dg(&dg, Some(&a), |_| {}).unwrap();
        for cg in [&a, &b] {
            w.add_channel(cg, None, |c| {
                c.data_type = DataType::FloatLE;
                c.bit_count = 64;
                c.name = Some("v".into());
            })
            .unwrap();
        }
        w.finalize().unwrap();

        let mut raw = bytes.borrow().clone();
        // dg_rec_id_size lives at DG+56; set it to 1 to declare record ids.
        let dg_off = sole_block(&raw, b"##DG");
        raw[dg_off + 56] = 1;

        let msg = parse_message(&open(raw).unwrap_err());
        assert!(msg.contains("unsorted"), "{msg}");
    }

    // -----------------------------------------------------------------
    // Malformed input must come back as `Err` — never as an abort
    // -----------------------------------------------------------------

    #[test]
    fn empty_input_is_an_error() {
        let err = open(Vec::new()).unwrap_err();
        let msg = parse_message(&err);
        assert!(msg.contains('0'), "{msg}");
        assert_eq!(err.code(), "MEASUREMENT_PARSE_FAILED");
    }

    #[test]
    fn garbage_bytes_quote_what_was_found() {
        let msg = parse_message(&open(vec![0xAB; 4096]).unwrap_err());
        assert!(msg.contains("Not an MDF4 file"), "{msg}");

        // Text that is valid UTF-8 but not an MDF identifier: the file's own
        // first bytes come back in the message.
        let mut text = b"hello, this is a text file and not a measurement at all.".to_vec();
        text.resize(4096, b' ');
        let msg = parse_message(&open(text).unwrap_err());
        assert!(msg.contains("hello, t"), "{msg}");
    }

    #[test]
    fn every_truncation_of_a_good_file_is_an_error_or_a_clean_read() {
        let full = two_group_file();
        for cut in 0..full.len() {
            let Ok(source) = open(full[..cut].to_vec()) else {
                continue;
            };
            // Anything that still opens must still answer coherently.
            let meta = source.metadata().unwrap();
            for group in &meta.groups {
                for channel in &group.channels {
                    if let Ok(data) = source.channel(group.index, &channel.name) {
                        assert_eq!(
                            data.time.len(),
                            data.values.len(),
                            "truncation at {cut}: ragged ChannelData"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_file_that_lies_about_its_record_count_is_refused() {
        // `Channel::values_as_f64` opens with `Vec::with_capacity(cn_cycle_count)`,
        // straight from the file. Before this guard a corrupt count asked the
        // allocator for terabytes and the process died — in a browser, the tab.
        let mut bytes = minimal_file();
        let cg = sole_block(&bytes, b"##CG");
        bytes[cg + 80..cg + 88].copy_from_slice(&u64::MAX.to_le_bytes());
        let msg = parse_message(&open(bytes).unwrap_err());
        assert!(msg.contains("cannot fit"), "{msg}");
    }

    #[test]
    fn a_corrupt_conversion_link_is_refused_before_the_reader_sees_it() {
        // `ConversionBlock::from_bytes` sizes a Vec from the block's own
        // `links_nr` and then indexes past the link section unchecked. Both
        // panic, inside `MDF::from_bytes`, where nothing downstream can catch
        // them under `panic = "abort"`.
        let base = two_group_file();

        // (a) an absurd link count
        let mut bytes = base.clone();
        let cc = find_block(&bytes, b"##CC");
        bytes[cc + 16..cc + 24].copy_from_slice(&u64::MAX.to_le_bytes());
        let msg = parse_message(&open(bytes).unwrap_err());
        assert!(msg.contains("link"), "{msg}");

        // (b) a plausible link count in a block far too short for it
        let mut bytes = base.clone();
        let cc = find_block(&bytes, b"##CC");
        bytes[cc + 8..cc + 16].copy_from_slice(&32u64.to_le_bytes());
        assert!(open(bytes).is_err());

        // (c) the conversion link pointing at the very end of the file
        let mut bytes = base.clone();
        let cn = find_block(&bytes, b"##CN");
        let past = bytes.len() as u64 - 8;
        bytes[cn + 56..cn + 64].copy_from_slice(&past.to_le_bytes());
        assert!(open(bytes).is_err());
    }

    #[test]
    fn a_looping_block_chain_is_refused() {
        let mut bytes = minimal_file();
        let dg = sole_block(&bytes, b"##DG");
        // Point the data group's "next" link back at itself.
        bytes[dg + 24..dg + 32].copy_from_slice(&(dg as u64).to_le_bytes());
        let msg = parse_message(&open(bytes).unwrap_err());
        assert!(msg.contains("loops back"), "{msg}");
    }

    #[test]
    fn mutated_bytes_never_abort_and_never_lie_about_lengths() {
        // The sweep that found both aborts, kept as a regression. A fixed LCG so
        // a failure names a reproducible round; the assertion is simply that
        // control comes back — a panic here *is* the defect.
        let base = two_group_file();
        let mut state: u64 = 0x005E_EDC0_FFEE;
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 11
        };

        for round in 0..4000u32 {
            let mut bytes = base.clone();
            for _ in 0..1 + next() % 8 {
                let i = (next() as usize) % bytes.len();
                bytes[i] = (next() & 0xFF) as u8;
            }
            if round % 5 == 0 {
                let cut = (next() as usize) % bytes.len();
                bytes.truncate(cut);
            }

            let Ok(source) = open(bytes) else { continue };
            let Ok(meta) = source.metadata() else {
                continue;
            };
            for group in &meta.groups {
                for channel in &group.channels {
                    if let Ok(data) = source.channel(group.index, &channel.name) {
                        assert_eq!(
                            data.time.len(),
                            data.values.len(),
                            "round {round}: ragged ChannelData"
                        );
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Message hygiene
    // -----------------------------------------------------------------

    #[test]
    fn messages_never_leak_a_build_path() {
        // `MdfError::TooShortBuffer` interpolates `file!()`, which is an
        // absolute path into the build machine's cargo registry. Nothing that
        // reaches a user may carry it.
        let truncated = describe(&MdfError::TooShortBuffer {
            actual: 10,
            expected: 168,
            file: "/home/someone/.cargo/registry/src/index/mf4-rs-3.6.0/src/lib.rs",
            line: 87,
        });
        assert!(!truncated.contains(".cargo"), "{truncated}");
        assert!(truncated.contains("168"), "{truncated}");

        let old = describe(&MdfError::FileVersioningError("300".to_string()));
        assert!(old.contains("MDF 3.00"), "{old}");
        assert!(old.contains("4.10"), "{old}");
    }

    // -----------------------------------------------------------------
    // The declared cycle count, which is what `mf4-rs` allocates from
    // -----------------------------------------------------------------

    /// Patch a group's declared record count and record size.
    ///
    /// `cn_cycle_count` sits at `##CG + 80` and `cg_data_bytes` at `##CG + 96`
    /// in the frozen MDF 4.1 layout; the writer emits exactly one `##CG`.
    fn claim_records(bytes: &mut [u8], cycles: u64, record_bytes: u32) {
        let cg = sole_block(bytes, b"##CG");
        bytes[cg + 80..cg + 88].copy_from_slice(&cycles.to_le_bytes());
        bytes[cg + 96..cg + 100].copy_from_slice(&record_bytes.to_le_bytes());
    }

    /// A group may declare any record count the file's *length* can cover, and
    /// trailing padding costs a byte per record of headroom. `values_as_f64`
    /// opens with `Vec::with_capacity(cn_cycle_count)` before decoding
    /// anything, so the declaration — not the records actually stored — is what
    /// gets allocated, twice (the channel and its time master).
    ///
    /// Measured before the guard: a 4 195 160-byte file holding **80** samples
    /// produced two vectors of 4 195 160 capacity, 67 MB in all. Scaled to the
    /// 512 MiB the wasm boundary will hold, that is an 8 GiB request against a
    /// 4 GiB address space — an allocator trap, and `panic = "abort"` there.
    ///
    /// The bound has to be the declared count, because `records[i]` is
    /// `min(declared, physical)` and a padded file's physical count is small.
    #[test]
    fn a_declared_cycle_count_far_above_the_records_present_is_refused() {
        // One byte per record is the worst case: the headroom `probe_records`
        // derives is `file_len / record_size`, so the padding *is* the claim.
        let mut bytes = minimal_file();
        bytes.resize(17 * 1024 * 1024, 0);
        let padded_len = bytes.len() as u64;
        claim_records(&mut bytes, padded_len, 1);

        let source = open(bytes).expect("a padded, over-declaring file still parses");
        // Metadata is unaffected — it reports what is physically there.
        assert_eq!(source.metadata().unwrap().groups[0].records, 80);

        let message = parse_message(&source.channel(0, "x").unwrap_err());
        assert!(message.contains(&padded_len.to_string()), "{message}");
        assert!(message.contains(&MAX_RECORDS.to_string()), "{message}");
        assert!(message.contains("shorter time range"), "{message}");
    }

    /// The guard must not fire on an over-declaration that is merely *wrong*
    /// rather than dangerous — a truncated recording over-declares by design,
    /// and `probe_records` already documents that the physical count wins.
    #[test]
    fn a_modest_over_declaration_still_reads_the_records_that_exist() {
        let mut bytes = minimal_file();
        claim_records(&mut bytes, 40, 16);
        let source = open(bytes).expect("an over-declaring file parses");
        let data = source.channel(0, "x").expect("and still reads");
        assert_eq!(data.len(), 5, "the five records actually written");
        assert_eq!(data.values, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
    }

    /// A chain of `count` overlapping `##DG` blocks, 40 bytes apart.
    ///
    /// Forty is the tightest spacing the format allows a *walkable* chain:
    /// every field [`validate_block_graph`] reads from a data group — the id,
    /// `block_len`, `links_nr`, the next-DG link at +24 and the first-CG link
    /// at +32 — lies inside the first 40 bytes, so the next block's id lands
    /// clear of all of them. Nothing requires blocks not to overlap, and the
    /// declared `block_len` (64, the minimum) never has to match the spacing.
    /// That is what makes the block ceiling cheap to reach: a million-block
    /// chain is 40 MB, well inside what a browser tab holds.
    fn overlapping_dg_chain(count: usize) -> Vec<u8> {
        const SPACING: u64 = 40;
        const FIRST_DG: u64 = 256;
        let mut b = vec![0u8; (FIRST_DG + SPACING * count as u64 + 256) as usize];
        b[..8].copy_from_slice(b"MDF     ");
        // ##HD at 64: 104 bytes, 6 links, the first of which is the data group.
        b[64..68].copy_from_slice(b"##HD");
        b[72..80].copy_from_slice(&104u64.to_le_bytes());
        b[80..88].copy_from_slice(&6u64.to_le_bytes());
        b[88..96].copy_from_slice(&FIRST_DG.to_le_bytes());
        for k in 0..count {
            let a = (FIRST_DG + SPACING * k as u64) as usize;
            b[a..a + 4].copy_from_slice(b"##DG");
            b[a + 8..a + 16].copy_from_slice(&64u64.to_le_bytes());
            b[a + 16..a + 24].copy_from_slice(&4u64.to_le_bytes());
            let next = if k + 1 < count {
                FIRST_DG + SPACING * (k + 1) as u64
            } else {
                0
            };
            b[a + 24..a + 32].copy_from_slice(&next.to_le_bytes());
            // No channel groups: the walk stays on the data-group chain.
            b[a + 32..a + 40].copy_from_slice(&0u64.to_le_bytes());
        }
        b
    }

    /// [`MAX_BLOCKS`] bounds how many blocks the pre-flight walk *visits*. It
    /// has to bound how much *work* the walk does, and with a linear membership
    /// test it did not: the cost was the square of the ceiling.
    ///
    /// Measured on the `Vec::contains` form, in release — 10 000 blocks 0.11 s,
    /// 40 000 blocks 1.00 s, 160 000 blocks **17.6 s**, textbook quadratic —
    /// which extrapolates to about eleven minutes at the ceiling this test
    /// drives. In a browser that is a worker wedged with no cancel button, on a
    /// 40 MB file the user merely dropped in. With a hash set the same walk is
    /// 26 ms in release and 0.3 s in debug at 160 000, and linear from there.
    ///
    /// The bound below is deliberately loose: it has two orders of magnitude of
    /// headroom over the fixed walk on a loaded machine, and the defect it
    /// guards against overruns it by a factor of thirty.
    #[test]
    fn a_million_block_chain_is_refused_in_seconds_not_minutes() {
        let bytes = overlapping_dg_chain(MAX_BLOCKS + 100);
        let start = std::time::Instant::now();
        let message = parse_message(&validate_block_graph(&bytes).unwrap_err());
        let elapsed = start.elapsed();
        assert!(message.contains(&MAX_BLOCKS.to_string()), "{message}");
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "walking the block ceiling took {elapsed:?}"
        );
    }

    /// The chain builder is only evidence if a *short* chain of the same shape
    /// really is walked rather than rejected on its first block.
    #[test]
    fn a_short_overlapping_chain_is_walked_to_its_end() {
        // No channel groups at all, so the walk succeeds and `open` then fails
        // for the honest reason.
        let bytes = overlapping_dg_chain(1_000);
        validate_block_graph(&bytes).expect("a thousand well-formed data groups");
        let message = parse_message(&open(bytes).unwrap_err());
        assert!(!message.contains("more than"), "{message}");
    }

    /// A chain that loops is still a cycle error, not a budget error — the hash
    /// set has to keep the diagnostic the `Vec` gave.
    #[test]
    fn a_looping_data_group_chain_is_named_as_a_loop() {
        let mut bytes = overlapping_dg_chain(4);
        // Point the last data group back at the first.
        let last = 256 + 40 * 3;
        bytes[last + 24..last + 32].copy_from_slice(&256u64.to_le_bytes());
        let message = parse_message(&validate_block_graph(&bytes).unwrap_err());
        assert!(message.contains("loops back on itself"), "{message}");
        assert!(message.contains("data group"), "{message}");
    }

    // -----------------------------------------------------------------
    // The storage chain, which is walked once per channel group
    // -----------------------------------------------------------------

    /// A file of `groups` channel groups whose data groups **all point at one
    /// shared chain** of `chain` empty `##DL` blocks appended to the end.
    ///
    /// Nothing in the format forbids that, and it is what makes the cost of
    /// `probe_records` the *product* of two numbers the file controls
    /// separately: a `##DL` block is 40 bytes, so the chain is cheap, and a
    /// channel group is a few hundred, so the repetition is cheap too.
    fn shared_storage_chain(groups: usize, chain: usize) -> Vec<u8> {
        let sink = SharedSink::default();
        let buf = sink.buf.clone();
        let mut w = MdfWriter::new_from_writer(sink);
        w.init_mdf_file().unwrap();
        for g in 0..groups {
            let cg = w.add_channel_group(None, |_| {}).unwrap();
            w.set_channel_group_name(&cg, &format!("g{g}")).unwrap();
            let t = w
                .add_channel(&cg, None, |c| {
                    c.data_type = DataType::FloatLE;
                    c.bit_count = 64;
                    c.name = Some("time".into());
                })
                .unwrap();
            w.set_time_channel(&t).unwrap();
            w.start_data_block_for_cg(&cg, 0).unwrap();
            w.write_record(&cg, &[DecodedValue::Float(0.0)]).unwrap();
            w.finish_data_block(&cg).unwrap();
        }
        w.finalize().unwrap();
        let mut b = buf.borrow().clone();

        let dl_start = b.len() as u64;
        b.resize(b.len() + chain * 40 + 64, 0);
        for k in 0..chain {
            let a = (dl_start + 40 * k as u64) as usize;
            b[a..a + 4].copy_from_slice(b"##DL");
            b[a + 8..a + 16].copy_from_slice(&40u64.to_le_bytes());
            b[a + 16..a + 24].copy_from_slice(&1u64.to_le_bytes());
            let next = if k + 1 < chain {
                dl_start + 40 * (k + 1) as u64
            } else {
                0
            };
            b[a + 24..a + 32].copy_from_slice(&next.to_le_bytes());
        }
        // Point every ##DG's data link (offset 40) at the chain head.
        let mut i = 0usize;
        while i + 4 <= b.len() {
            if &b[i..i + 4] == b"##DG" {
                b[i + 40..i + 48].copy_from_slice(&dl_start.to_le_bytes());
                i += 4;
            } else {
                i += 1;
            }
        }
        b
    }

    /// `probe_records` walks a data group's storage chain once per channel
    /// group, inside `mf4-rs`, where there is no budget. So the work is the
    /// *product* of the chain length and the group count, both of which the
    /// file sets independently and cheaply.
    ///
    /// Measured before [`check_storage_chain`] existed, in release: 10 groups
    /// over 20 000 blocks took 0.10 s, 40 groups the same chain 0.23 s, and 40
    /// groups over 80 000 blocks 0.95 s — linear in the product. A 40 MB file
    /// (a thousand groups, a million blocks) is about five minutes, and the
    /// boundary's 512 MiB limit buys hours, on the call the user makes by
    /// dropping a file on the page. After the guard the same file is refused in
    /// 0.12 s, and the ceiling on the work is [`MAX_BLOCKS`] rather than the
    /// file's size squared.
    #[test]
    fn a_storage_chain_walked_once_per_group_is_bounded_by_the_block_budget() {
        // 40 × 20 000 blocks is 1.6 M visits against a 1 M budget.
        let bytes = shared_storage_chain(40, 20_000);
        let start = std::time::Instant::now();
        let message = parse_message(&open(bytes).unwrap_err());
        let elapsed = start.elapsed();
        assert!(message.contains(&MAX_BLOCKS.to_string()), "{message}");
        assert!(elapsed < std::time::Duration::from_secs(30), "{elapsed:?}");
    }

    /// …and a chain that fits the budget is still read, so the guard is a
    /// ceiling rather than a ban on fragmented storage.
    #[test]
    fn a_fragmented_chain_within_the_budget_still_opens() {
        let source = open(shared_storage_chain(4, 2_000)).expect("4 × 2 000 fits");
        let meta = source.metadata().expect("metadata");
        assert_eq!(meta.groups.len(), 4);
    }

    // -----------------------------------------------------------------
    // The independent oracle: a file this toolchain did not write
    // -----------------------------------------------------------------

    /// The one committed `.mf4` in the parent repo, byte-for-byte.
    ///
    /// Every other fixture in this file is round-tripped through `mf4-rs`'s own
    /// writer, which is a real round trip but a *self-consistent* one: if the
    /// writer and the reader shared a misreading of the spec, all of them would
    /// still pass. This file was written by **asammdf** (its `##ID` block says
    /// `amdf8.8.`), the reference Python implementation, and read here by a
    /// Rust crate that has never met it — so agreement is evidence about the
    /// format rather than about `mf4-rs`.
    ///
    /// It is also the *same bytes* the Java oracle asserts against, copied from
    /// `../frEES/backend/core/src/test/resources/measurement/`. The expectations
    /// below are `Mf4SpikeTest.gate1*`'s, transcribed — not re-derived — so a
    /// divergence from the engine this port replaces shows up as a failure here.
    ///
    /// `include_bytes!` inside `#[cfg(test)]` costs the shipped wasm bundle
    /// nothing: the module is not compiled into a release build, which matters
    /// because that bundle is already over budget (`docs/status-phase9.md`).
    fn real_asammdf_file() -> Vec<u8> {
        include_bytes!("../../../../fixtures/measurement/a_small_uncompressed.mf4").to_vec()
    }

    /// `Mf4SpikeTest.gate1MetadataEnumerates`, assertion for assertion.
    #[test]
    fn a_real_asammdf_file_enumerates_as_the_java_did() {
        let source = open(real_asammdf_file()).expect("a real MDF 4.10 recording opens");
        let meta = source.metadata().unwrap();

        assert_eq!(meta.groups.len(), 1);
        let group = &meta.groups[0];
        assert_eq!(group.records, 1000);
        // The time master plus speed / torque / valve_open.
        assert_eq!(group.channels.len(), 4);
        assert_eq!(group.channels.iter().filter(|c| c.time_master).count(), 1);

        let speed = group
            .channels
            .iter()
            .find(|c| c.name == "speed")
            .expect("a speed channel");
        assert_eq!(speed.unit.as_deref(), Some("m/s"));
        assert_eq!(speed.kind, ChannelKind::Analog);
        assert_eq!(speed.kind.as_str(), "analog");

        // asammdf writes no `cg_tx_acq_name`, so the positional fallback is what
        // the user sees — the Java's `getName().orElse("group " + index)`.
        assert_eq!(group.name, "group 0");
    }

    /// `Mf4SpikeTest.gate1ChannelExtractsWithSpike`, assertion for assertion.
    ///
    /// The generator injects a single-sample spike of 99.5 at `t = 5.00 s` into
    /// an otherwise smooth `20 + 10·sin(2t)`. It is here because a decimator
    /// that loses it loses the only interesting sample in the recording, but for
    /// this file it does something simpler: it pins one sample to an exact index,
    /// so an off-by-one in the record walk cannot pass.
    #[test]
    fn a_real_asammdf_channel_carries_the_javas_numbers() {
        let source = open(real_asammdf_file()).unwrap();
        let data = source.channel(0, "speed").unwrap();

        assert_eq!(data.time.len(), 1000);
        assert_eq!(data.values.len(), 1000);
        assert_eq!(data.time[0], 0.0);
        assert!((data.time[1] - data.time[0] - 0.01).abs() < 1e-9);
        for i in 1..1000 {
            assert!(data.time[i] > data.time[i - 1], "time must increase at {i}");
        }

        assert!(
            (data.values[500] - 99.5).abs() < 1e-9,
            "{}",
            data.values[500]
        );
        // The neighbour still follows the underlying signal, which is what makes
        // the sample above a spike rather than a decoding slip.
        let expected = 20.0 + 10.0 * (2.0 * data.time[499]).sin();
        assert!(
            (data.values[499] - expected).abs() < 1e-9,
            "got {}, want {expected}",
            data.values[499]
        );
    }

    /// `Mf4SpikeTest.gate1MissingChannelIsATypedError` — plus the part the Java
    /// did not do, which is naming the channels that *are* there.
    #[test]
    fn a_missing_channel_in_a_real_file_is_typed_and_lists_the_alternatives() {
        let source = open(real_asammdf_file()).unwrap();
        let err = source.channel(0, "no_such_channel").unwrap_err();
        let msg = notfound_message(&err);
        assert!(msg.contains("no_such_channel"), "{msg}");
        assert!(msg.contains("\"speed\""), "{msg}");
        assert!(msg.contains("\"torque\""), "{msg}");
    }

    /// The claim [`kind_of`] makes about asammdf, tested against asammdf.
    ///
    /// `valve_open` is a `uint8` that only ever holds 0 or 1 — semantically a
    /// boolean, and the sort of channel the 0..1 domain rule exists to catch.
    /// asammdf does not set `cn_flags` bit 3, so the rule cannot fire and the
    /// channel is reported `Analog`. That is not a defect being papered over:
    /// it is exactly what the Java answered, and the alternative is decoding
    /// every sample of every channel during `metadata()` to guess. Asserted
    /// rather than left implicit so that a future widening of the rule has to
    /// come here and argue with it.
    #[test]
    fn an_asammdf_zero_one_channel_stays_analog_as_it_did_in_java() {
        let source = open(real_asammdf_file()).unwrap();
        let meta = source.metadata().unwrap();
        let valve = meta.groups[0]
            .channels
            .iter()
            .find(|c| c.name == "valve_open")
            .expect("a valve_open channel");
        assert_eq!(valve.kind, ChannelKind::Analog);

        // ...and the samples really are only ever 0 or 1, so the channel is the
        // case the rule is about rather than a badly chosen example.
        let data = source.channel(0, "valve_open").unwrap();
        assert_eq!(data.len(), 1000);
        assert!(data.values.iter().all(|v| *v == 0.0 || *v == 1.0));
        assert!(data.values.contains(&1.0));
    }

    /// Truncating a *real* file must degrade the same way a synthetic one does.
    ///
    /// The synthetic sweep above walks a file `mf4-rs` laid out itself, with its
    /// own block ordering; asammdf orders blocks differently, so this re-runs
    /// the same contract over a layout the pre-flight has not been tuned to.
    #[test]
    fn every_truncation_of_the_real_file_is_an_error_or_a_clean_read() {
        let full = real_asammdf_file();
        // Every 64th cut: the file is 26 KB and the walk is the point, not the
        // byte count.
        for cut in (0..full.len()).step_by(64) {
            let Ok(source) = open(full[..cut].to_vec()) else {
                continue;
            };
            let Ok(meta) = source.metadata() else {
                continue;
            };
            for group in &meta.groups {
                for channel in &group.channels {
                    if let Ok(data) = source.channel(group.index, &channel.name) {
                        assert_eq!(
                            data.time.len(),
                            data.values.len(),
                            "truncation at {cut}: ragged ChannelData"
                        );
                    }
                }
            }
        }
    }

    /// A decoded channel must not retain the declared count's worth of memory
    /// behind a short `len()`. The wasm boundary sizes its read cache from
    /// `data.len()`, so a vector carrying 8000× its length in spare capacity
    /// would be cached as if it were nothing.
    #[test]
    fn a_decoded_channel_retains_no_more_than_its_samples() {
        let mut bytes = minimal_file();
        bytes.resize(bytes.len() + 64 * 1024, 0);
        claim_records(&mut bytes, 4_000, 16);
        let source = open(bytes).unwrap();
        let data = source.channel(0, "x").unwrap();
        assert_eq!(data.len(), 5);
        assert!(
            data.values.capacity() <= 8 && data.time.capacity() <= 8,
            "retained {} / {} slots for 5 samples",
            data.values.capacity(),
            data.time.capacity()
        );
    }
}
