//! Where the card's time goes in the title's lists, in every build
//! (`VICTORY_CARD_PROFILE=0`: not measured): a timestamp wherever a list
//! changes what it runs (a pipeline of the title's, a clamp of indirect
//! arguments of ours, our passes as the list closes), read once the list
//! has run, and the time between two stamps given to what ran there; said
//! with the frame line, the pipelines that took the most named.
//!
//! Run 2026-11-27 21:31 (16 GB, turbo): with translating no longer the
//! bound, the frames took 165-180 ms, the card about 97 % busy, and nothing
//! said what it was busy with.
//!
//! A stamp is written when the card reaches it in the list; draws that
//! overlap blur where one pipeline's time ends and the next one's begins,
//! and a clamp's time holds the waits its barriers make. Stamps live in one
//! ring of slots that lists take blocks of; a list whose stamps were
//! written over before it was read (or not in order) is left out, and
//! counted.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

use super::{HostList, host};

/// Kinds of what runs between a stamp and the next.
pub(crate) const START: u8 = 0;
pub(crate) const PIPELINE: u8 = 1;
pub(crate) const CLAMP: u8 = 2;
pub(crate) const CLOSE: u8 = 3;
pub(crate) const END: u8 = 4;
/// A list of a submission's own steps (all of it, whatever its stamps).
const OWN: usize = 5;

/// Slots of the ring, and of a block a list takes at a time.
const SLOTS: u32 = 1 << 17;
/// After them, slots in pairs for lists of ours ([`other_begin`]).
const OTHER_SLOTS: u32 = 4096;
const BLOCK: u32 = 64;
/// Stamps a list makes at most (the last, its end, always).
const MOST: usize = 8192;

struct Ring {
    heap: ID3D12QueryHeap,
    _back: ID3D12Resource,
    /// The read-back buffer's mapping: a tick count a slot.
    data: usize,
    /// Ticks a second.
    frequency: u64,
}

// SAFETY: free-threaded objects and a mapping only read, of slots the card
// wrote for lists that have run.
unsafe impl Send for Ring {}
// SAFETY: as above.
unsafe impl Sync for Ring {}

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_CARD_PROFILE").is_none_or(|v| v != "0"))
}

fn ring() -> Option<&'static Ring> {
    static RING: OnceLock<Option<Ring>> = OnceLock::new();
    if !on() {
        return None;
    }
    RING.get_or_init(|| {
        let made = (|| -> windows::core::Result<Ring> {
            let host = host().ok_or_else(windows::core::Error::empty)?;
            let mut heap: Option<ID3D12QueryHeap> = None;
            let mut back: Option<ID3D12Resource> = None;
            let mut data = std::ptr::null_mut();
            // SAFETY: plain object creation and a mapping of our own buffer.
            unsafe {
                host.device.CreateQueryHeap(
                    &D3D12_QUERY_HEAP_DESC { Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP, Count: SLOTS + OTHER_SLOTS, NodeMask: 0 },
                    &mut heap,
                )?;
                host.device.CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_READBACK, ..Default::default() },
                    D3D12_HEAP_FLAG_NONE,
                    &D3D12_RESOURCE_DESC {
                        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                        Width: u64::from(SLOTS + OTHER_SLOTS) * 8,
                        Height: 1,
                        DepthOrArraySize: 1,
                        MipLevels: 1,
                        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                        ..Default::default()
                    },
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut back,
                )?;
                let back = back.ok_or_else(windows::core::Error::empty)?;
                back.Map(0, None, Some(&mut data))?;
                let frequency = host.queue.GetTimestampFrequency()?;
                Ok(Ring { heap: heap.ok_or_else(windows::core::Error::empty)?, _back: back, data: data as usize, frequency })
            }
        })();
        match made {
            Ok(ring) => Some(ring),
            Err(e) => {
                tracing::warn!(error = %e, "the card's time in lists is not measured");
                None
            }
        }
    })
    .as_ref()
}

/// A list's stamps since its last reset.
pub(crate) struct Marks {
    /// The block being filled: its first slot, and how many are taken.
    block: Option<(u32, u32)>,
    /// Blocks filled before it.
    blocks: Vec<(u32, u32)>,
    /// Each stamp: its slot, the kind of what runs from it on, and the
    /// pipeline (kind [`PIPELINE`]).
    marks: Vec<(u32, u8, usize)>,
    /// The pipelines' names, by pipeline, as this recording first bound them.
    names: HashMap<usize, String>,
    /// The kind and pipeline of the last stamp.
    last: (u8, usize),
    /// Sent to the card, and as a list of a submission's own steps.
    submitted: bool,
    own: bool,
}

impl Default for Marks {
    fn default() -> Self {
        Marks {
            block: None,
            blocks: Vec::new(),
            marks: Vec::new(),
            names: HashMap::new(),
            last: (u8::MAX, 0),
            submitted: false,
            own: false,
        }
    }
}

impl Marks {
    fn slot(&mut self) -> u32 {
        if let Some((start, used)) = &mut self.block
            && *used < BLOCK
        {
            *used += 1;
            return *start + *used - 1;
        }
        if let Some(full) = self.block.take() {
            self.blocks.push(full);
        }
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let start = NEXT.fetch_add(BLOCK, Relaxed) % SLOTS;
        self.block = Some((start, 1));
        start
    }
}

/// Since the last line, in ticks: by kind ([`START`] .. [`OWN`]); lists
/// counted, stamps, lists left out, clamps.
static BY_KIND: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
static COUNTS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
/// Ticks and stamps by pipeline, with its name.
static BY_PIPELINE: Mutex<Option<HashMap<usize, (u64, u64, String)>>> = Mutex::new(None);

impl HostList {
    /// What runs from here on in the list is of `kind` (and pipeline `pso`).
    pub(crate) fn card_mark(&self, kind: u8, pso: usize) {
        let Some(ring) = ring() else {
            return;
        };
        let mut m = self.card.lock().unwrap_or_else(|e| e.into_inner());
        if kind != END && ((kind, pso) == m.last || m.marks.len() >= MOST) {
            return;
        }
        let slot = m.slot();
        // SAFETY: an open list and our query heap; the slot is in it.
        unsafe { self.list.EndQuery(&ring.heap, D3D12_QUERY_TYPE_TIMESTAMP, slot) };
        m.marks.push((slot, kind, pso));
        m.last = (kind, pso);
    }

    /// A pipeline of the title's runs from here on.
    pub(crate) fn card_pipeline(&self, pso: usize, name: &str) {
        if ring().is_none() {
            return;
        }
        {
            let mut m = self.card.lock().unwrap_or_else(|e| e.into_inner());
            if m.last == (PIPELINE, pso) {
                return;
            }
            if !m.names.contains_key(&pso) {
                m.names.insert(pso, name.to_owned());
            }
        }
        self.card_mark(PIPELINE, pso);
    }

    /// The list closes: its last stamp, and all of them resolved into the
    /// ring's read-back buffer.
    pub(crate) fn card_end(&self) {
        let Some(ring) = ring() else {
            return;
        };
        self.card_mark(END, 0);
        let m = self.card.lock().unwrap_or_else(|e| e.into_inner());
        for &(start, used) in m.blocks.iter().chain(m.block.iter()) {
            // SAFETY: an open list; our heap and read-back buffer, the slots
            // in both.
            unsafe {
                self.list.ResolveQueryData(
                    &ring.heap,
                    D3D12_QUERY_TYPE_TIMESTAMP,
                    start,
                    used,
                    &ring._back,
                    u64::from(start) * 8,
                )
            };
        }
    }

    /// The list is sent to the card (`own`: as a list of a submission's own
    /// steps).
    pub(crate) fn card_submitted(&self, own: bool) {
        if ring().is_none() {
            return;
        }
        let mut m = self.card.lock().unwrap_or_else(|e| e.into_inner());
        m.submitted = true;
        m.own |= own;
    }

    /// The list has run (it is being reset): its stamps counted, and taken.
    pub(crate) fn card_account(&self) {
        let Some(ring) = ring() else {
            return;
        };
        let m = std::mem::take(&mut *self.card.lock().unwrap_or_else(|e| e.into_inner()));
        if !m.submitted || m.marks.len() < 2 || m.marks.last().map(|l| l.1) != Some(END) {
            return;
        }
        // SAFETY: slots of the mapping, which the card wrote for this list
        // (it has run) unless another list's took them since.
        let ticks: Vec<u64> =
            m.marks.iter().map(|&(slot, ..)| unsafe { ((ring.data as *const u64).add(slot as usize)).read_volatile() }).collect();
        let total = ticks[ticks.len() - 1].wrapping_sub(ticks[0]);
        if ticks.windows(2).any(|w| w[1] < w[0]) || total > ring.frequency.saturating_mul(2) {
            COUNTS[2].fetch_add(1, Relaxed);
            return;
        }
        COUNTS[0].fetch_add(1, Relaxed);
        COUNTS[1].fetch_add(m.marks.len() as u64, Relaxed);
        if m.own {
            BY_KIND[OWN].fetch_add(total, Relaxed);
            return;
        }
        let mut by: HashMap<usize, (u64, u64)> = HashMap::new();
        let mut kinds = [0u64; 5];
        for (i, &(_, kind, pso)) in m.marks.iter().enumerate().take(m.marks.len() - 1) {
            let took = ticks[i + 1] - ticks[i];
            kinds[usize::from(kind.min(4))] += took;
            if kind == CLAMP {
                COUNTS[3].fetch_add(1, Relaxed);
            }
            if kind == PIPELINE {
                let e = by.entry(pso).or_default();
                e.0 += took;
                e.1 += 1;
            }
        }
        for (k, ticks) in kinds.iter().enumerate() {
            if *ticks > 0 {
                BY_KIND[k].fetch_add(*ticks, Relaxed);
            }
        }
        if by.is_empty() {
            return;
        }
        let mut all = BY_PIPELINE.lock().unwrap_or_else(|e| e.into_inner());
        let all = all.get_or_insert_with(HashMap::new);
        for (pso, (ticks, stamps)) in by {
            let e = all.entry(pso).or_insert_with(|| (0, 0, m.names.get(&pso).cloned().unwrap_or_default()));
            e.0 += ticks;
            e.1 += stamps;
        }
    }
}

/// Lists of ours outside the title's, timed whole: those that bring copies
/// in the card's memory up to date before a submission (sends and hash
/// passes), and those that compare them after it.
pub(crate) const SYNC: u8 = 0;
pub(crate) const CHECK: u8 = 1;
/// Their stamps not read yet: the list, its kind, the first of its pair.
static OTHER: Mutex<Vec<(usize, u8, u32)>> = Mutex::new(Vec::new());
/// Since the last line: ticks and lists, by kind.
static OTHER_BY: [[AtomicU64; 2]; 2] = [const { [const { AtomicU64::new(0) }; 2] }; 2];

/// One of those lists of ours (`kind`) begins (it was just reset, its last
/// run over): what that run took is counted, and a stamp begins this one.
pub(crate) fn other_begin(list: &ID3D12GraphicsCommandList, kind: u8) {
    let Some(ring) = ring() else {
        return;
    };
    let key = list.as_raw() as usize;
    let before = {
        let mut pending = OTHER.lock().unwrap_or_else(|e| e.into_inner());
        pending.iter().position(|e| e.0 == key).map(|i| pending.swap_remove(i))
    };
    if let Some((_, was, slot)) = before {
        // SAFETY: the pair's slots of the mapping, written by the list's
        // last run, which is over.
        let [from, to] = [slot, slot + 1].map(|s| unsafe { ((ring.data as *const u64).add(s as usize)).read_volatile() });
        if to >= from && to - from <= ring.frequency.saturating_mul(2) {
            OTHER_BY[usize::from(was.min(1))][0].fetch_add(to - from, Relaxed);
            OTHER_BY[usize::from(was.min(1))][1].fetch_add(1, Relaxed);
        }
    }
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let slot = SLOTS + NEXT.fetch_add(2, Relaxed) % OTHER_SLOTS;
    // SAFETY: an open list and our query heap; the slot is in it.
    unsafe { list.EndQuery(&ring.heap, D3D12_QUERY_TYPE_TIMESTAMP, slot) };
    OTHER.lock().unwrap_or_else(|e| e.into_inner()).push((key, kind, slot));
}

/// The list [`other_begin`] began is about to close: its last stamp, and
/// both resolved.
pub(crate) fn other_end(list: &ID3D12GraphicsCommandList) {
    let Some(ring) = ring() else {
        return;
    };
    let key = list.as_raw() as usize;
    let Some(slot) = OTHER.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|e| e.0 == key).map(|e| e.2) else {
        return;
    };
    // SAFETY: an open list; our heap and read-back buffer, the slots in both.
    unsafe {
        list.EndQuery(&ring.heap, D3D12_QUERY_TYPE_TIMESTAMP, slot + 1);
        list.ResolveQueryData(&ring.heap, D3D12_QUERY_TYPE_TIMESTAMP, slot, 2, &ring._back, u64::from(slot) * 8);
    }
}

/// With the frame line, per frame over `frames`.
pub(crate) fn line(frames: f64) {
    let Some(ring) = ring() else {
        return;
    };
    let kinds: [u64; 6] = std::array::from_fn(|i| BY_KIND[i].swap(0, Relaxed));
    let counts: [u64; 4] = std::array::from_fn(|i| COUNTS[i].swap(0, Relaxed));
    let by = BY_PIPELINE.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let other: [[u64; 2]; 2] = std::array::from_fn(|k| std::array::from_fn(|i| OTHER_BY[k][i].swap(0, Relaxed)));
    if frames <= 0.0 || (counts[0] == 0 && counts[2] == 0) {
        return;
    }
    let ms = |ticks: u64| ticks as f64 * 1e3 / ring.frequency.max(1) as f64 / frames;
    let total: u64 = kinds.iter().sum();
    let mut top: Vec<(usize, (u64, u64, String))> = by.into_iter().collect();
    top.sort_unstable_by(|a, b| b.1.0.cmp(&a.1.0));
    let pipelines = top.len();
    let text: Vec<String> = top
        .iter()
        .take(12)
        .map(|(_, (ticks, stamps, name))| {
            let name: String = name.chars().take(120).collect();
            format!("{name} {:.2} ms ({:.0}x)", ms(*ticks), *stamps as f64 / frames)
        })
        .collect();
    tracing::info!(
        "the card's time in the title's lists per frame (VICTORY_CARD_PROFILE=0: not measured): {:.1} ms in {:.1} lists ({:.0} \
         stamps; {:.1} lists left out, their stamps written over); the title's pipelines {:.1} ms ({pipelines} of them), the title's \
         work before its first pipeline {:.1} ms, our clamps of indirect arguments {:.1} ms ({:.0} of them), our passes as a list \
         closes {:.1} ms, our lists of the submissions' own steps {:.1} ms; outside the title's lists: copies in the card's memory \
         brought up to date (sends and hash passes) {:.1} ms ({:.1} lists), compared after submissions {:.1} ms ({:.1} lists); the \
         costliest pipelines: {}",
        ms(total),
        counts[0] as f64 / frames,
        counts[1] as f64 / frames,
        counts[2] as f64 / frames,
        ms(kinds[PIPELINE as usize]),
        ms(kinds[START as usize]),
        ms(kinds[CLAMP as usize]),
        counts[3] as f64 / frames,
        ms(kinds[CLOSE as usize]),
        ms(kinds[OWN]),
        ms(other[0][0]),
        other[0][1] as f64 / frames,
        ms(other[1][0]),
        other[1][1] as f64 / frames,
        text.join("; "),
    );
}

/// A list's pipeline, as [`HostList::card_pipeline`] keys it.
pub(crate) fn key(pso: &ID3D12PipelineState) -> usize {
    pso.as_raw() as usize
}
