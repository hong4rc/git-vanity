/// Nonce generation strategy.
///
/// Each thread gets a unique base + thread_id offset.
/// Nonces use bytes in range 0x80–0xFF to stay non-printable
/// and avoid 0x0A (newline) which would break the header.
const NONCE_LEN: usize = 10;

/// Generate a nonce from a counter value and thread ID.
/// Maps all bytes into the safe range 0x80–0xFF using bitwise OR.
#[inline(always)]
pub fn generate_nonce(counter: u64, thread_id: u16) -> [u8; NONCE_LEN] {
    let mut nonce = [0x80u8; NONCE_LEN];

    // First 2 bytes: thread ID (mapped to 0x80-0xFF)
    nonce[0] = 0x80 | ((thread_id >> 8) as u8 & 0x7F);
    nonce[1] = 0x80 | (thread_id as u8 & 0x7F);

    // Remaining 8 bytes: counter (mapped to 0x80-0xFF via iterator)
    counter
        .to_le_bytes()
        .iter()
        .enumerate()
        .for_each(|(i, &b)| nonce[i + 2] = 0x80 | (b & 0x7F));

    nonce
}

/// Get the nonce length.
pub const fn nonce_len() -> usize {
    NONCE_LEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_no_newline() {
        (0..1000u64).for_each(|i| {
            let nonce = generate_nonce(i, 0);
            assert!(!nonce.contains(&0x0A), "Nonce contains newline");
            assert!(nonce.iter().all(|&b| b >= 0x80), "Nonce has printable byte");
        });
    }

    #[test]
    fn test_nonce_different_threads() {
        let a = generate_nonce(0, 0);
        let b = generate_nonce(0, 1);
        assert_ne!(a, b);
    }

    #[test]
    fn test_nonce_different_counters() {
        let a = generate_nonce(0, 0);
        let b = generate_nonce(1, 0);
        assert_ne!(a, b);
    }
}
