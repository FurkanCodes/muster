//! Bytes as lowercase hexadecimal: tokens, ids and checksums.

use std::fmt::Write as _;

/// `bytes` as two lowercase hex digits each.
pub fn encode(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    #[test]
    fn each_byte_is_two_lowercase_digits() {
        assert_eq!(super::encode(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
        assert_eq!(super::encode(&[]), "");
    }
}
