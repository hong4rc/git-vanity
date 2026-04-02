use sha1::{Digest, Sha1};

/// Hash a complete Git object and return the raw 20-byte SHA-1.
#[inline]
#[cfg(test)]
pub fn hash_git_object(object: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(object);
    hasher.finalize().into()
}

/// Pre-computed SHA-1 state for incremental hashing.
///
/// Strategy pattern: pre-compute the hash state up to the nonce position,
/// then clone + finalize for each attempt. This avoids re-hashing the
/// (constant) prefix on every single attempt — a major speedup.
#[derive(Clone)]
pub struct IncrementalHasher {
    /// SHA-1 state after hashing: "commit <len>\0<headers>\nx-nonce "
    prefix_state: Sha1,
    /// The suffix bytes: "\n\n<message>"
    suffix: Vec<u8>,
}

impl IncrementalHasher {
    /// Build an incremental hasher from pre-computed commit parts.
    ///
    /// `prefix` = header lines ending with "x-nonce "
    /// `suffix` = "\n\n<message>"
    /// `nonce_len` = fixed nonce byte length
    pub fn new(prefix: &[u8], suffix: &[u8], nonce_len: usize) -> Self {
        let total_content_len = prefix.len() + nonce_len + suffix.len();
        let git_header = format!("commit {}\0", total_content_len);

        let mut prefix_state = Sha1::new();
        prefix_state.update(git_header.as_bytes());
        prefix_state.update(prefix);

        IncrementalHasher {
            prefix_state,
            suffix: suffix.to_vec(),
        }
    }

    /// Hash with a specific nonce. Clones the pre-computed state,
    /// feeds nonce + suffix, and finalizes.
    #[inline]
    pub fn hash_with_nonce(&self, nonce: &[u8]) -> [u8; 20] {
        let mut hasher = self.prefix_state.clone();
        hasher.update(nonce);
        hasher.update(&self.suffix);
        hasher.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_known_value() {
        // Empty blob for reference
        let object = b"blob 0\0";
        let hash = hash_git_object(object);
        let hex = hex::encode(hash);
        assert_eq!(hex, "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn test_incremental_matches_full() {
        let prefix = b"tree abc\nauthor A <a@b> 1 +0\ncommitter B <b@c> 1 +0\nx-nonce ";
        let nonce = [0x80u8, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89];
        let suffix = b"\n\nMsg\n";

        // Full object hash
        let total_content_len = prefix.len() + nonce.len() + suffix.len();
        let git_header = format!("commit {}\0", total_content_len);
        let mut full = Vec::new();
        full.extend_from_slice(git_header.as_bytes());
        full.extend_from_slice(prefix);
        full.extend_from_slice(&nonce);
        full.extend_from_slice(suffix);
        let expected = hash_git_object(&full);

        // Incremental hash
        let inc = IncrementalHasher::new(prefix, suffix, nonce.len());
        let actual = inc.hash_with_nonce(&nonce);

        assert_eq!(expected, actual);
    }
}
