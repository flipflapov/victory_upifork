//! Diagnostics: a timeline of a few frames. `VICTORY_TIMELINE=<file>` writes
//! one, from the first present after `VICTORY_TIMELINE_AT` seconds (60) on
//! and for `VICTORY_TIMELINE_FRAMES` frames (3);
//! `VICTORY_TIMELINE=<from s>[+<from s>..],<frames>,<file>` one for each of
//! the times (each in a file of its own, the second in its name): a Chrome trace (JSON, for
//! `chrome://tracing` or Perfetto) and beside it a text summary (`.txt`)
//! with the chain of work and waits each frame's present came out of.
//!
//! What is recorded while it runs, by every thread:
//! - spans of ours ([`span`]): the title's calls, the submitting thread's
//!   stages, the relay's, the pools' jobs, texture and streaming work;
//! - waits and the wake-ups that end them ([`hooks`]): the functions the
//!   title's threads wait with (its imports are bound to wrappers) and those
//!   our own code does (the entries of our own import table: every contended
//!   lock, condition and park of the standard library is a `WaitOnAddress`),
//!   each with the thread that woke it and where it waited;
//! - kernel calls on the address space that took long;
//! - the card: when each submission began and ended on it (timestamps).
//!
//! Without the variable nothing is wrapped and a span is one load of a
//! flag; a build without `diagnostics` has nothing of it.

#[cfg(all(windows, feature = "diagnostics"))]
mod hooks;
#[cfg(all(windows, feature = "diagnostics"))]
mod record;
#[cfg(all(windows, feature = "diagnostics"))]
mod report;

#[cfg(all(windows, feature = "diagnostics"))]
pub use hooks::import;
#[cfg(all(windows, feature = "diagnostics"))]
pub use record::{Span, span};
#[cfg(all(windows, feature = "diagnostics"))]
pub(crate) use record::{called, card_wait, on, polled, presented, sent, ticks, title_thread, told};

/// A span of ours: from where it is made until it is dropped. Without the
/// timeline, the spans of the submitting thread's stages and a few others
/// (see [`profile`]) are summed by name for the frame line's profile.
#[cfg(not(all(windows, feature = "diagnostics")))]
pub struct Span(Option<(&'static str, std::time::Instant)>);

#[cfg(not(all(windows, feature = "diagnostics")))]
impl Span {
    /// The span ends here and another begins.
    #[inline]
    pub fn next(&mut self, name: &'static str) {
        if let Some((was, at)) = self.0.take() {
            let now = std::time::Instant::now();
            profile::add(was, now.duration_since(at));
            self.0 = profile::wanted(name).then_some((name, now));
        }
    }

    /// The span ends here.
    #[inline]
    pub fn end(self) {}
}

#[cfg(not(all(windows, feature = "diagnostics")))]
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((name, at)) = self.0.take() {
            profile::add(name, at.elapsed());
        }
    }
}

/// A span named `name`.
#[cfg(not(all(windows, feature = "diagnostics")))]
#[inline]
pub fn span(name: &'static str) -> Span {
    Span(profile::wanted(name).then(|| (name, std::time::Instant::now())))
}

/// Where the submitting thread's time goes, in every build: the spans of
/// its stages ("submit: ..."), the relay's landings ("landing: ..."), the
/// heap pass, the heaps' records, settling flights, refreshes in front and
/// the waits of the line, summed by name between frame lines and said with
/// them (`VICTORY_PROFILE=0`: not). Spans that run thousands of times a
/// frame (a fault, a step, a copy's refresh) are left out: two clock reads
/// and a lock each. After the submit-thread profiler of Neptune by Dolberth
/// (Dolberthi/neptune-dolberth-nvidia-8gb `prof.rs`), here on the spans the
/// timeline already has.
#[cfg(not(all(windows, feature = "diagnostics")))]
pub(crate) mod profile {
    use std::sync::Mutex;
    use std::sync::OnceLock;

    static SUMS: Mutex<Vec<(&'static str, u64, u64)>> = Mutex::new(Vec::new());

    fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("VICTORY_PROFILE").is_none_or(|v| v != "0"))
    }

    /// Whether a span of this name is summed.
    #[inline]
    pub(crate) fn wanted(name: &'static str) -> bool {
        const KEPT: [&str; 10] = [
            "submit:",
            "landing:",
            "the heap pass",
            "the heaps' records",
            "settling flights",
            "refreshes in front",
            "refreshes of copies",
            "a present waits",
            "a submission waits",
            "copies in the card's memory",
        ];
        on() && KEPT.iter().any(|k| name.starts_with(k))
    }

    pub(super) fn add(name: &'static str, took: std::time::Duration) {
        let ns = took.as_nanos() as u64;
        let mut sums = SUMS.lock().unwrap_or_else(|e| e.into_inner());
        match sums.iter_mut().find(|(n, ..)| std::ptr::eq(*n, name) || *n == name) {
            Some((_, all, times)) => {
                *all += ns;
                *times += 1;
            }
            None => sums.push((name, ns, 1)),
        }
    }

    /// The profile since the last line, per frame over `frames`: the
    /// steps that took the most first.
    pub(crate) fn line(frames: f64) {
        if !on() || frames <= 0.0 {
            return;
        }
        let mut sums = std::mem::take(&mut *SUMS.lock().unwrap_or_else(|e| e.into_inner()));
        if sums.is_empty() {
            return;
        }
        sums.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        let text: Vec<String> = sums
            .iter()
            .take(18)
            .map(|(name, ns, times)| format!("{name} {:.1} ms ({:.1}x)", *ns as f64 / 1e6 / frames, *times as f64 / frames))
            .collect();
        tracing::info!("where our threads' time goes per frame (VICTORY_PROFILE=0: not said): {}", text.join("; "));
    }
}
