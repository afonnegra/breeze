//! ClipboardManager (FR-06): snapshot, set_text and restore of the Windows
//! clipboard so that text injection (FR-05) can paste transcribed text
//! without destroying whatever the user had copied (text, images, files).
//!
//! All clipboard access goes through a RAII guard (open with retry and
//! backoff, close guaranteed on drop). Snapshots capture CF_UNICODETEXT,
//! CF_DIB and CF_HDROP as opaque HGLOBAL byte blobs, which is enough to
//! round-trip text, images and file lists.

use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};

// Standard clipboard format ids from winuser.h. The windows crate takes raw
// u32 format ids in Get/SetClipboardData, so local consts avoid pulling the
// whole Win32_System_Ole feature just for these three values.
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

/// Formats captured by snapshot(), in the order restore() re-inserts them.
/// All three are HGLOBAL-backed, so raw byte copies round-trip.
const SNAPSHOT_FORMATS: [u32; 3] = [CF_UNICODETEXT, CF_DIB, CF_HDROP];

const OPEN_RETRY_ATTEMPTS: u32 = 5;
const OPEN_RETRY_BASE_MS: u64 = 10;

/// Errors surfaced by the clipboard manager.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("could not open clipboard after retries")]
    OpenTimeout,
    #[error("clipboard read failed - {0}")]
    Read(String),
    #[error("clipboard write failed - {0}")]
    Write(String),
    #[error("global memory allocation failed - {0}")]
    Alloc(String),
}

/// Opaque copy of the clipboard contents for the formats we care about.
/// An empty snapshot (empty clipboard) is valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardSnapshot {
    /// (format id, raw HGLOBAL bytes) per captured format.
    formats: Vec<(u32, Vec<u8>)>,
}

impl ClipboardSnapshot {
    /// True when no supported format was present on the clipboard.
    pub fn is_empty(&self) -> bool {
        self.formats.is_empty()
    }

    /// Captured (format id, bytes) pairs.
    pub fn formats(&self) -> &[(u32, Vec<u8>)] {
        &self.formats
    }
}

/// Exponential backoff schedule for OpenClipboard retries. Pure so the
/// sequence itself is unit-testable without touching the real clipboard.
fn backoff_delays(attempts: u32, base_ms: u64) -> Vec<u64> {
    (0..attempts).map(|i| base_ms << i).collect()
}

/// CF_UNICODETEXT payload for a text - UTF-16 LE bytes plus the mandatory
/// null terminator. Pure so it is unit-testable.
fn text_to_clipboard_payload(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0u16))
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

/// RAII guard over the global clipboard lock. Construction retries
/// OpenClipboard with backoff (another process may hold the clipboard);
/// Drop always calls CloseClipboard, so no early return can leak the lock.
struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Result<Self, ClipboardError> {
        for delay_ms in backoff_delays(OPEN_RETRY_ATTEMPTS, OPEN_RETRY_BASE_MS) {
            // SAFETY: OpenClipboard(None) is safe to call from any thread;
            // failure only means another process currently holds the lock.
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Self);
            }
            thread::sleep(Duration::from_millis(delay_ms));
        }
        Err(ClipboardError::OpenTimeout)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: the guard only exists after a successful OpenClipboard, so
        // this thread owns the clipboard lock. The result is ignored - there
        // is no meaningful recovery from CloseClipboard failing.
        let _ = unsafe { CloseClipboard() };
    }
}

/// Reads one HGLOBAL-backed format as raw bytes. Returns Ok(None) when the
/// format is not present. The caller must hold the ClipboardGuard.
fn read_format(format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
    // SAFETY: clipboard is open (guard held by the caller per contract).
    if unsafe { IsClipboardFormatAvailable(format) }.is_err() {
        return Ok(None);
    }
    // SAFETY: clipboard is open; the returned handle is owned by the system
    // and must not be freed by us.
    let handle = unsafe { GetClipboardData(format) }
        .map_err(|e| ClipboardError::Read(format!("GetClipboardData {format} - {e}")))?;
    let hglobal = HGLOBAL(handle.0);
    // SAFETY: for CF_UNICODETEXT, CF_DIB and CF_HDROP the handle is an
    // HGLOBAL; locking it yields a readable pointer of GlobalSize bytes.
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return Err(ClipboardError::Read(format!(
            "GlobalLock failed for format {format}"
        )));
    }
    // SAFETY: hglobal is locked and valid, so GlobalSize reports its size.
    let size = unsafe { GlobalSize(hglobal) };
    // SAFETY: ptr is valid for size bytes while the lock is held.
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, size) }.to_vec();
    // SAFETY: pairs with the GlobalLock above. GlobalUnlock reports an error
    // when the lock count reaches zero, which is the expected outcome here,
    // so the result is deliberately ignored.
    let _ = unsafe { GlobalUnlock(hglobal) };
    Ok(Some(bytes))
}

/// Allocates an HGLOBAL, copies bytes into it and hands it to the clipboard.
/// The caller must hold the ClipboardGuard.
///
/// OWNERSHIP - if SetClipboardData succeeds the system owns the HGLOBAL and
/// it must NOT be freed by us; if it fails we still own it and MUST free it.
fn write_format(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
    // SAFETY: GMEM_MOVEABLE is required for clipboard allocations. A zero
    // size is bumped to 1 because GlobalLock on a zero-size block fails.
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }
        .map_err(|e| ClipboardError::Alloc(format!("GlobalAlloc of {} bytes - {e}", bytes.len())))?;
    // SAFETY: hglobal is a valid allocation we own at this point.
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        // SAFETY: the allocation never reached the clipboard - free it.
        let _ = unsafe { GlobalFree(Some(hglobal)) };
        return Err(ClipboardError::Alloc(
            "GlobalLock on fresh allocation failed".to_string(),
        ));
    }
    // SAFETY: destination was allocated with at least bytes.len() capacity
    // and source and destination do not overlap.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len()) };
    // SAFETY: pairs with GlobalLock above; result ignored as in read_format.
    let _ = unsafe { GlobalUnlock(hglobal) };
    // SAFETY: clipboard is open (guard held by the caller per contract).
    match unsafe { SetClipboardData(format, Some(HANDLE(hglobal.0))) } {
        Ok(_) => Ok(()),
        Err(e) => {
            // SAFETY: SetClipboardData failed, so ownership of the HGLOBAL
            // stayed with us - free it to avoid a leak.
            let _ = unsafe { GlobalFree(Some(hglobal)) };
            Err(ClipboardError::Write(format!("SetClipboardData {format} - {e}")))
        }
    }
}

/// Captures the current clipboard contents (text, image, file list) so they
/// can be restored after we paste the transcription.
pub fn snapshot() -> Result<ClipboardSnapshot, ClipboardError> {
    let _guard = ClipboardGuard::open()?;
    let mut formats = Vec::new();
    for &format in &SNAPSHOT_FORMATS {
        if let Some(bytes) = read_format(format)? {
            formats.push((format, bytes));
        }
    }
    Ok(ClipboardSnapshot { formats })
}

/// Replaces the clipboard contents with the given text (CF_UNICODETEXT).
pub fn set_text(text: &str) -> Result<(), ClipboardError> {
    let payload = text_to_clipboard_payload(text);
    let _guard = ClipboardGuard::open()?;
    // SAFETY: clipboard is open via the guard above.
    unsafe { EmptyClipboard() }
        .map_err(|e| ClipboardError::Write(format!("EmptyClipboard - {e}")))?;
    write_format(CF_UNICODETEXT, &payload)
}

/// Puts a previously captured snapshot back on the clipboard. An empty
/// snapshot restores an empty clipboard.
pub fn restore(snap: &ClipboardSnapshot) -> Result<(), ClipboardError> {
    let _guard = ClipboardGuard::open()?;
    // SAFETY: clipboard is open via the guard above.
    unsafe { EmptyClipboard() }
        .map_err(|e| ClipboardError::Write(format!("EmptyClipboard - {e}")))?;
    for (format, bytes) in &snap.formats {
        write_format(*format, bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- unit tests (no real clipboard) ----

    #[test]
    fn backoff_sequence_is_five_doubling_delays() {
        let delays = backoff_delays(OPEN_RETRY_ATTEMPTS, OPEN_RETRY_BASE_MS);
        assert_eq!(delays, vec![10, 20, 40, 80, 160]);
    }

    #[test]
    fn text_payload_is_utf16le_with_null_terminator() {
        let payload = text_to_clipboard_payload("ab");
        assert_eq!(payload, vec![0x61, 0x00, 0x62, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn text_payload_encodes_non_bmp_as_surrogate_pair() {
        // U+1F600 encodes as the surrogate pair D83D DE00 in UTF-16.
        let payload = text_to_clipboard_payload("\u{1F600}");
        assert_eq!(payload, vec![0x3D, 0xD8, 0x00, 0xDE, 0x00, 0x00]);
    }

    #[test]
    fn text_payload_of_empty_string_is_just_the_terminator() {
        assert_eq!(text_to_clipboard_payload(""), vec![0x00, 0x00]);
    }

    #[test]
    fn snapshot_struct_preserves_formats_and_clones() {
        let snap = ClipboardSnapshot {
            formats: vec![(CF_UNICODETEXT, vec![1, 2]), (CF_HDROP, vec![3])],
        };
        assert!(!snap.is_empty());
        assert_eq!(snap.formats().len(), 2);
        assert_eq!(snap.formats()[0], (CF_UNICODETEXT, vec![1, 2]));
        let copy = snap.clone();
        assert_eq!(copy, snap);
        let empty = ClipboardSnapshot { formats: Vec::new() };
        assert!(empty.is_empty());
    }

    // ---- integration tests against the REAL system clipboard ----
    // The clipboard is global state, so these are ignored by default and
    // must run single-threaded (cargo test --lib clipboard -- --ignored
    // --nocapture --test-threads=1). They preserve and restore whatever
    // the user had copied.

    /// Decodes the CF_UNICODETEXT entry of a fresh snapshot, if present.
    fn read_clipboard_text() -> Option<String> {
        let snap = snapshot().expect("snapshot for readback");
        let bytes = snap
            .formats()
            .iter()
            .find(|(format, _)| *format == CF_UNICODETEXT)
            .map(|(_, bytes)| bytes.clone())?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let end = units.iter().position(|&unit| unit == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }

    /// Builds a minimal, valid CF_DIB payload entirely in memory - a
    /// BITMAPINFOHEADER describing a 2x2 32bpp bottom-up bitmap followed
    /// by 16 bytes of BGRA pixels. No GDI objects involved: CF_DIB is
    /// just an HGLOBAL blob with this layout, which is exactly the
    /// generic HGLOBAL path that snapshot and restore exercise.
    fn synthetic_dib() -> Vec<u8> {
        let mut dib = Vec::with_capacity(40 + 16);
        dib.extend_from_slice(&40u32.to_le_bytes()); // biSize (BITMAPINFOHEADER)
        dib.extend_from_slice(&2i32.to_le_bytes()); // biWidth
        dib.extend_from_slice(&2i32.to_le_bytes()); // biHeight (bottom-up)
        dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        dib.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
        dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
        dib.extend_from_slice(&16u32.to_le_bytes()); // biSizeImage
        dib.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
        dib.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
        dib.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
        dib.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
        // Four distinct BGRX pixels so a byte shuffle would be caught.
        dib.extend_from_slice(&[
            0x00, 0x00, 0xFF, 0x00, // red
            0x00, 0xFF, 0x00, 0x00, // green
            0xFF, 0x00, 0x00, 0x00, // blue
            0xFF, 0xFF, 0xFF, 0x00, // white
        ]);
        dib
    }

    /// Test helper - empties the clipboard and places one format on it
    /// through the module's own guard + write_format (real
    /// SetClipboardData, same path production restore() uses).
    fn put_format(format: u32, bytes: &[u8]) {
        let _guard = ClipboardGuard::open().expect("open clipboard for test");
        // SAFETY: clipboard is open via the guard above.
        unsafe { EmptyClipboard() }.expect("EmptyClipboard for test");
        write_format(format, bytes).expect("write_format for test");
    }

    /// Returns the CF_DIB bytes of a snapshot, if present.
    fn dib_bytes(snap: &ClipboardSnapshot) -> Option<Vec<u8>> {
        snap.formats()
            .iter()
            .find(|(format, _)| *format == CF_DIB)
            .map(|(_, bytes)| bytes.clone())
    }

    #[test]
    #[ignore]
    fn clipboard_text_roundtrip() {
        // Courtesy - preserve whatever the user currently has copied.
        let user_snap = snapshot().expect("snapshot of user clipboard");

        set_text("inputvoice test").expect("set_text");
        assert_eq!(read_clipboard_text().as_deref(), Some("inputvoice test"));

        restore(&user_snap).expect("restore of user clipboard");
        let after = snapshot().expect("snapshot after restore");
        assert_eq!(after.formats(), user_snap.formats());
    }

    #[test]
    #[ignore]
    fn clipboard_snapshot_restores_text() {
        let user_snap = snapshot().expect("snapshot of user clipboard");

        set_text("A").expect("set_text A");
        let snap_a = snapshot().expect("snapshot of A");
        set_text("B").expect("set_text B");
        restore(&snap_a).expect("restore of A");
        assert_eq!(read_clipboard_text().as_deref(), Some("A"));

        restore(&user_snap).expect("restore of user clipboard");
    }

    /// CF_DIB is the format that justifies the opaque-HGLOBAL snapshot
    /// design (FR-06 - a copied image must survive an injection), so it
    /// gets its own roundtrip: put a synthetic DIB on the clipboard,
    /// snapshot it, overwrite with text (what inject() does), restore,
    /// and verify the DIB came back byte for byte.
    #[test]
    #[ignore]
    fn clipboard_dib_roundtrip() {
        let user_snap = snapshot().expect("snapshot of user clipboard");

        let dib = synthetic_dib();
        put_format(CF_DIB, &dib);

        let snap_dib = snapshot().expect("snapshot of DIB");
        assert_eq!(
            dib_bytes(&snap_dib).as_deref(),
            Some(dib.as_slice()),
            "snapshot did not capture the DIB byte for byte"
        );

        set_text("X").expect("set_text X");
        assert_eq!(read_clipboard_text().as_deref(), Some("X"));

        restore(&snap_dib).expect("restore of DIB snapshot");
        let after = snapshot().expect("snapshot after restore");
        assert_eq!(
            dib_bytes(&after).as_deref(),
            Some(dib.as_slice()),
            "restored DIB differs from the original bytes"
        );

        restore(&user_snap).expect("restore of user clipboard");
    }
}
