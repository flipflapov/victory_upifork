//! What shaders write into title memory the title asked for as
//! write-combined is not copied back into it (`VICTORY_WC_NO_BACK=1`; off by
//! default). After crossducking/RAZDA4A-XEN (GPL-3.0) `wc.rs`, trimmed to the
//! switch, the ranges it needs and its line.
//!
//! The console's CPU writes such memory but does not read it back, as PC
//! titles keep their per-frame rings in memory they only write (their run
//! belorx_8: all 46 MB a frame of the copy-back went there; with the switch
//! 27 -> 0.5 MB a frame, preparing a frame 45.8 -> 32-35 ms, on a 6-core PC
//! with 16 GB). XMem says every protection the title asks for
//! ([`title_memory_protection`]); a declared write whose range lies wholly
//! in write-combined memory, is not small and is of a copy in the card's
//! memory whose memory is write-watched is not declared at all (no
//! snapshot, no compare, no copy back): the copy keeps it
//! ([`crate::shadow::Shadow::card_owns`]), and a refresh that cannot tell
//! what the CPU wrote leaves it out.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

/// Kinds of title memory, by what the title asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// No protection was ever noted there.
    Unknown,
    /// Cached for the CPU.
    Cached,
    /// The console's write-combined bit (0x400).
    WriteCombined,
    /// The console's uncached bit (0x200).
    Uncached,
}

/// The console's protection bits for how the CPU reaches memory.
const UNCACHED: u32 = 0x200;
const WRITE_COMBINED: u32 = 0x400;

/// `VICTORY_WC_NO_BACK=1`: see the module.
pub(crate) fn no_back() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_WC_NO_BACK").is_some_and(|v| v == "1"))
}

/// Ranges under `VICTORY_WC_NO_BACK_MIN_KB` (64 by default) are copied back
/// as before: the few results the title's CPU does read are small.
fn no_back_min() -> usize {
    static MIN: OnceLock<usize> = OnceLock::new();
    *MIN.get_or_init(|| {
        std::env::var("VICTORY_WC_NO_BACK_MIN_KB").ok().and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(64) << 10
    })
}

/// Since the last line: declarations left to the card and their bytes;
/// small ones kept and their bytes; refreshes that left such ranges out (the
/// memory not watched) and their bytes; bytes kept because the copy is not
/// known to be watched, and because it is not in the card's memory.
static SKIPPED: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];

/// Whether copies the card keeps ranges of send up only the dwords the
/// title changed (`VICTORY_WC_KEEP_STORES=1`; off: the parts written since,
/// whole, as for any copy).
pub(crate) fn keep_stores() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_WC_KEEP_STORES").is_some_and(|v| v == "1"))
}

/// A refresh left out `bytes` the card keeps, the memory not being watched.
pub(crate) fn unwatched(bytes: u64) {
    SKIPPED[4].fetch_add(1, Ordering::Relaxed);
    SKIPPED[5].fetch_add(bytes, Ordering::Relaxed);
}

/// Whether shaders' writes to `[address, address + len)` stay on the card:
/// the whole range lies in write-combined title memory and is not small,
/// and its copy is in the card's memory (`carded`) and known to be
/// write-watched (`watched`). Others are copied back as before.
pub(crate) fn skip_back(address: usize, len: usize, carded: bool, watched: bool) -> bool {
    if !no_back() || len == 0 {
        return false;
    }
    if kind(address) != Kind::WriteCombined || kind(address + len - 1) != Kind::WriteCombined {
        return false;
    }
    if len < no_back_min() {
        SKIPPED[2].fetch_add(1, Ordering::Relaxed);
        SKIPPED[3].fetch_add(len as u64, Ordering::Relaxed);
        return false;
    }
    if !carded || !watched {
        SKIPPED[if carded { 6 } else { 7 }].fetch_add(len as u64, Ordering::Relaxed);
        return false;
    }
    SKIPPED[0].fetch_add(1, Ordering::Relaxed);
    SKIPPED[1].fetch_add(len as u64, Ordering::Relaxed);
    true
}

/// A set of disjoint `[start, end)` ranges, joined where they touch.
#[derive(Default)]
pub(crate) struct Ranges(BTreeMap<usize, usize>);

impl Ranges {
    pub(crate) fn add(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let (mut from, mut to) = (from, to);
        // One that starts before and reaches `from` is joined.
        if let Some((&s, &e)) = self.0.range(..=from).next_back()
            && e >= from
        {
            if e >= to {
                return;
            }
            from = s;
        }
        let inside: Vec<usize> = self.0.range(from..=to).map(|(&s, _)| s).collect();
        for s in inside {
            if let Some(e) = self.0.remove(&s) {
                to = to.max(e);
            }
        }
        self.0.insert(from, to);
    }

    /// The ranges that overlap `[from, to)`.
    pub(crate) fn overlapping(&self, from: usize, to: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        let first = self.0.range(..from).next_back().filter(|(_, e)| **e > from).map(|(&s, &e)| (s, e));
        first.into_iter().chain(self.0.range(from..to).map(|(&s, &e)| (s, e)))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Start of a range -> (end, kind); ranges never overlap.
fn ranges() -> &'static RwLock<BTreeMap<usize, (usize, Kind)>> {
    static RANGES: OnceLock<RwLock<BTreeMap<usize, (usize, Kind)>>> = OnceLock::new();
    RANGES.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn kind_of_protection(protect: u32) -> Kind {
    if protect & WRITE_COMBINED != 0 {
        Kind::WriteCombined
    } else if protect & UNCACHED != 0 {
        Kind::Uncached
    } else {
        Kind::Cached
    }
}

/// The title asked for `[at, at + len)` with the console's `protect`
/// (reserved, committed or allocated): the newest word on that range.
pub fn title_memory_protection(at: usize, len: usize, protect: u32) {
    if !no_back() || at == 0 || len == 0 {
        return;
    }
    insert(&mut ranges().write().unwrap_or_else(|e| e.into_inner()), at, at.saturating_add(len), kind_of_protection(protect));
}

/// Puts `[from, to)` as `kind`, cutting what it overlaps.
fn insert(map: &mut BTreeMap<usize, (usize, Kind)>, from: usize, to: usize, kind: Kind) {
    // A range that starts before `from` and reaches into it keeps its head
    // (and its tail past `to`, if it reaches past).
    if let Some((&start, &(end, k))) = map.range(..from).next_back()
        && end > from
    {
        map.insert(start, (from, k));
        if end > to {
            map.insert(to, (end, k));
        }
    }
    // Ranges that start inside go; the last may keep its tail.
    let inside: Vec<usize> = map.range(from..to).map(|(&s, _)| s).collect();
    for start in inside {
        if let Some((end, k)) = map.remove(&start)
            && end > to
        {
            map.insert(to, (end, k));
        }
    }
    map.insert(from, (to, kind));
}

pub(crate) fn kind(address: usize) -> Kind {
    let map = ranges().read().unwrap_or_else(|e| e.into_inner());
    match map.range(..=address).next_back() {
        Some((_, &(end, k))) if address < end => k,
        _ => Kind::Unknown,
    }
}

/// With the frame line, per frame over `frames` (nothing while it is off).
pub(crate) fn line(frames: f64) {
    if !no_back() || frames <= 0.0 {
        return;
    }
    let s: [f64; 8] = std::array::from_fn(|i| SKIPPED[i].swap(0, Ordering::Relaxed) as f64 / frames);
    tracing::info!(
        "shaders' writes into write-combined title memory left on the card (VICTORY_WC_NO_BACK, under {} KiB kept), per frame: \
         {:.0} declarations, {:.1} MB declared (a range each time it is declared); small ones copied back as before: {:.0}, \
         {:.1} MB; refreshes that left them out, the memory not watched: {:.1}, {:.2} MB; copied back as before, the copy not \
         known to be watched: {:.1} MB, not in the card's memory: {:.1} MB",
        no_back_min() >> 10,
        s[0],
        s[1] / 1e6,
        s[2],
        s[3] / 1e6,
        s[4],
        s[5] / 1e6,
        s[6] / 1e6,
        s[7] / 1e6,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_protection_wins_and_cuts_older_ranges() {
        let mut map = BTreeMap::new();
        insert(&mut map, 0x1000, 0x9000, Kind::Cached);
        insert(&mut map, 0x3000, 0x5000, Kind::WriteCombined);
        let got: Vec<_> = map.iter().map(|(&s, &(e, k))| (s, e, k)).collect();
        assert_eq!(
            got,
            [(0x1000, 0x3000, Kind::Cached), (0x3000, 0x5000, Kind::WriteCombined), (0x5000, 0x9000, Kind::Cached)]
        );
    }

    #[test]
    fn ranges_join_and_answer_overlaps() {
        let mut r = Ranges::default();
        r.add(10, 20);
        r.add(30, 40);
        r.add(20, 25);
        r.add(12, 15);
        assert_eq!(r.0.iter().map(|(&s, &e)| (s, e)).collect::<Vec<_>>(), [(10, 25), (30, 40)]);
        assert_eq!(r.overlapping(0, 11).collect::<Vec<_>>(), [(10, 25)]);
        assert_eq!(r.overlapping(25, 30).collect::<Vec<_>>(), Vec::<(usize, usize)>::new());
        assert_eq!(r.overlapping(24, 31).collect::<Vec<_>>(), [(10, 25), (30, 40)]);
    }

    #[test]
    fn protection_bits_pick_the_kind() {
        assert_eq!(kind_of_protection(0x2404), Kind::WriteCombined);
        assert_eq!(kind_of_protection(0x2204), Kind::Uncached);
        assert_eq!(kind_of_protection(0x22004), Kind::Cached);
    }
}
