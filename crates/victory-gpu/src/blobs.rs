//! Bytes the layer cannot use yet, written into the log as text
//! (`VICTORY_BLOBS_LOG=1`, off by default; the UPI launcher sets it), so that
//! a log sent from a player's PC carries what is needed to work on them
//! offline: the containers of shaders that do not translate
//! (`crate::pipeline`) and what the title hands the pipeline creation
//! methods that are not implemented (`crate::device`'s 0x1c8 and 0x218).
//!
//! Each blob is one or more lines `blob <name> <part>/<parts> <bytes>: <base64>`
//! (standard alphabet, padded, 3,000 bytes of the blob a line); joined in
//! order and decoded they are the blob. At most [`BUDGET`] bytes of blobs
//! a run, and a name once.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bytes of blobs a run writes at most.
const BUDGET: usize = 4 << 20;
/// Bytes of a blob a line.
const LINE: usize = 3000;

static SPENT: AtomicUsize = AtomicUsize::new(0);
static NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Whether blobs are written (`VICTORY_BLOBS_LOG=1`).
pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("VICTORY_BLOBS_LOG").is_ok_and(|v| v.trim() == "1"))
}

/// Writes `bytes` as the blob `name` (once a name, while the budget lasts);
/// whether it was written.
pub(crate) fn log(name: &str, bytes: &[u8]) -> bool {
    if !on() || bytes.is_empty() {
        return false;
    }
    {
        let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
        if names.iter().any(|n| n == name) {
            return false;
        }
        if SPENT.fetch_add(bytes.len(), Ordering::Relaxed) + bytes.len() > BUDGET {
            return false;
        }
        names.push(name.to_owned());
    }
    let parts = bytes.len().div_ceil(LINE);
    for (i, piece) in bytes.chunks(LINE).enumerate() {
        tracing::info!("blob {name} {}/{parts} {}: {}", i + 1, bytes.len(), base64(piece));
    }
    true
}

/// `bytes` in base64 (standard alphabet, padded).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let b = [group[0], *group.get(1).unwrap_or(&0), *group.get(2).unwrap_or(&0)];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= group.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(super::base64(&[0xff, 0xfe, 0xfd, 0x00]), "//79AA==");
    }
}
