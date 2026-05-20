//! Helpers for the 16-byte MD5 checksums osu! uses for beatmap identity.

/// Type alias for an MD5 checksum (16 raw bytes).
pub type Md5 = [u8; 16];

/// Sentinel "no checksum" value used when the source had an empty / missing hash.
/// MD5 collisions on all-zero are cryptographically negligible; we treat all-zero
/// as a sentinel rather than adding an `Option` wrapper (saves 8 B per entry under
/// alignment).
pub const EMPTY: Md5 = [0u8; 16];

/// Parse a 32-char lowercase or uppercase hex string into 16 raw bytes.
/// Returns `EMPTY` for empty input. Returns `None` for malformed hex.
pub fn parse_hex(s: &str) -> Option<Md5> {
    if s.is_empty() {
        return Some(EMPTY);
    }
    if s.len() != 32 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = [0u8; 16];
    for i in 0..16 {
        let hi = hex_digit(bytes[2 * i])?;
        let lo = hex_digit(bytes[2 * i + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

/// Convert 16 raw bytes back to a 32-char lowercase hex string.
pub fn to_hex(md5: Md5) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for b in md5 {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// `true` iff the checksum equals the sentinel `EMPTY`.
#[inline]
pub fn is_empty(md5: &Md5) -> bool {
    *md5 == EMPTY
}
