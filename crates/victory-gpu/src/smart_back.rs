//! Demand-driven GPU-resident copy-back engine (`VICTORY_SMART_COPY_BACK=1`).
//!
//! From Dolberthi/neptune-dolberth-nvidia-8gb (Neptune HyperEngine by
//! Dolberth, GPL-3.0-or-later), `crates/victory-gpu/src/smart_back.rs` at
//! 658d561; hooked here where the copy-back joins declared ranges (the card
//! plan, the relay's and the submission's hand-back) and where the title's
//! copies and fills are recorded. In this tree a title copy or fill that
//! reads a card copy also has its range fetched for itself (`looked`, the
//! card plan's picked ranges), skipped ranges or not.
//!
//! By default, Victory hashes, fetches over PCIe, and word-merges back to title
//! RAM every buffer range that any shader wrote (30-62 MB/frame, 50-111 ms/frame).
//! In modern Scarlett titles, >90% of large shader-written buffers (skinning, particles, culling,
//! tile lighting, decals, indirect pools) are GPU->GPU only: later GPU shaders
//! already read the live bytes straight from VRAM (`Shadow::card`), and with
//! `VICTORY_COPY_CHANGED=1` later CPU refreshes only upload bytes the CPU itself
//! changed without clobbering GPU-resident outputs.
//!
//! When `VICTORY_SMART_COPY_BACK=1` is enabled:
//! - Small buffers (`<= VICTORY_SMART_COPY_BACK_KB`, default 256 KiB: occlusion
//!   queries, counters, stats, small readbacks) ALWAYS copy back normally.
//! - Large buffers (`> threshold`) stay GPU-resident (skipping GPU hash-compare
//!   dispatches, PCIe copy-queue fetches, and CPU word-merges) unless a title
//!   copy/fill reads from their address range, in which case they are
//!   auto-promoted permanently to full copy-back.
//! - Texture read-backs (`read_backs`), bottom-of-pipe writes, and fence signals
//!   are completely untouched.
//! - `VICTORY_SMART_COPY_BACK=0` (or unset) restores full copy-back instantly.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

use crate::shadow::Shadow;

/// Whether demand-driven smart copy-back is active (`VICTORY_SMART_COPY_BACK=1`).
pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_SMART_COPY_BACK").is_some_and(|v| v == "1"))
}

/// Buffers at or below this byte size always copy back (`VICTORY_SMART_COPY_BACK_KB`,
/// default 256 KiB; 0 keeps only promoted buffers).
fn threshold_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("VICTORY_SMART_COPY_BACK_KB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map_or(256 << 10, |kb| kb.min(65_536) << 10)
    })
}

/// Large GPU-written shadows promoted to full copy-back after a title read hit them.
static PROMOTED: OnceLock<RwLock<HashSet<u64>>> = OnceLock::new();
/// Address spans of large GPU-resident shadows `(start, end, id)` for fast overlap lookup.
static TRACKED: OnceLock<RwLock<Vec<(usize, usize, u64)>>> = OnceLock::new();
/// Fast deduplication set of shadow IDs already in `TRACKED`.
static KNOWN: OnceLock<RwLock<HashSet<u64>>> = OnceLock::new();

/// Telemetry: `[skipped_ranges, skipped_bytes, copied_ranges, promoted_count]`.
static STATS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
/// Size distribution of shader-written shadows per frame: `[<=4K, 4-32K, 32-256K, >256K]`.
static BUCKETS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

#[inline]
fn record_bucket(len: u64) {
    let idx = if len <= 4 << 10 {
        0
    } else if len <= 32 << 10 {
        1
    } else if len <= 256 << 10 {
        2
    } else {
        3
    };
    BUCKETS[idx].fetch_add(1, Ordering::Relaxed);
}

fn promoted() -> &'static RwLock<HashSet<u64>> {
    PROMOTED.get_or_init(|| RwLock::new(HashSet::new()))
}

fn tracked() -> &'static RwLock<Vec<(usize, usize, u64)>> {
    TRACKED.get_or_init(|| RwLock::new(Vec::new()))
}

fn known() -> &'static RwLock<HashSet<u64>> {
    KNOWN.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Whether `shadow`'s shader-written ranges should be compared, fetched over
/// PCIe, and merged back into title RAM.
pub(crate) fn should_copy_back(shadow: &Shadow) -> bool {
    if !enabled() {
        return true;
    }
    let (start, len) = shadow.span();
    record_bucket(len);
    if len <= threshold_bytes() {
        STATS[2].fetch_add(1, Ordering::Relaxed);
        return true;
    }
    let id = shadow.id();
    if promoted()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&id)
    {
        STATS[2].fetch_add(1, Ordering::Relaxed);
        return true;
    }
    if !known()
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&id)
    {
        let mut k = known().write().unwrap_or_else(|e| e.into_inner());
        if k.insert(id) {
            let start = start as usize;
            let end = start.saturating_add(len as usize);
            tracked()
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .push((start, end, id));
        }
    }
    STATS[0].fetch_add(1, Ordering::Relaxed);
    STATS[1].fetch_add(len, Ordering::Relaxed);
    false
}

/// Called when a title copy/fill reads `[src, src + len)` of title memory:
/// any large GPU-resident shadow overlapping that range is permanently promoted
/// to full copy-back.
pub(crate) fn note_cpu_read(src: usize, len: usize) {
    if !enabled() || len == 0 {
        return;
    }
    let end = src.saturating_add(len);
    let mut hits: Vec<u64> = Vec::new();
    {
        let spans = tracked().read().unwrap_or_else(|e| e.into_inner());
        for &(s, e, id) in spans.iter() {
            if s < end && src < e {
                hits.push(id);
            }
        }
    }
    if hits.is_empty() {
        return;
    }
    let mut prom = promoted().write().unwrap_or_else(|e| e.into_inner());
    for id in hits {
        if prom.insert(id) {
            let total = STATS[3].fetch_add(1, Ordering::Relaxed) + 1;
            tracing::info!(
                shadow_id = id,
                src = format_args!("{src:#x}"),
                len,
                total_promoted = total,
                "smart copy-back: promoted GPU buffer after title read"
            );
        }
    }
}

/// Periodic frame telemetry line.
pub(crate) fn log_stats(frames: f64) {
    if !enabled() || frames <= 0.0 {
        return;
    }
    let skipped = STATS[0].swap(0, Ordering::Relaxed) as f64 / frames;
    let mb = STATS[1].swap(0, Ordering::Relaxed) as f64 / (1024.0 * 1024.0) / frames;
    let copied = STATS[2].swap(0, Ordering::Relaxed) as f64 / frames;
    let prom = STATS[3].load(Ordering::Relaxed);
    let [b4, b32, b256, bbig] = [0, 1, 2, 3].map(|i| BUCKETS[i].swap(0, Ordering::Relaxed) as f64 / frames);
    if skipped + copied > 0.0 {
        tracing::info!(
            "smart copy-back (VICTORY_SMART_COPY_BACK=1): {skipped:.1} ranges/frame kept GPU-resident ({mb:.1} MB), \
             {copied:.1} copied back (<= {} KiB or promoted), {prom} auto-promoted [<=4K:{b4:.0} 4-32K:{b32:.0} 32-256K:{b256:.0} >256K:{bbig:.0}]",
            threshold_bytes() >> 10
        );
    }
}
