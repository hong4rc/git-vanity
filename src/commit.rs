/// Parse and reconstruct Git commit objects.
///
/// A commit object looks like:
/// ```text
/// tree <hash>\n
/// parent <hash>\n          (0 or more)
/// author <name> <email> <timestamp> <tz>\n
/// committer <name> <email> <timestamp> <tz>\n
/// [gpgsig <sig>]\n         (optional, multiline)
/// [other-header value]\n   (optional)
/// \n
/// <commit message>
/// ```

#[derive(Debug, Clone)]
pub struct CommitObject {
    /// Raw header lines (everything before the blank line)
    pub header_lines: Vec<String>,
    /// Commit message (everything after the blank line)
    pub message: String,
    /// Whether this commit had a gpgsig header
    pub had_signature: bool,
}

/// State machine for folding over header lines.
/// Encodes the GPG signature skip as a pure state transition.
enum FoldState {
    Normal,
    InSignature,
}

impl CommitObject {
    /// Parse a raw commit object using fold to process header lines
    /// as a state machine (Normal | InSignature) with no mutable flags.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let (header_part, message) = raw
            .find("\n\n")
            .map(|pos| (&raw[..pos], &raw[pos + 2..]))
            .ok_or("Invalid commit object: no blank line separating header and message")?;

        let (header_lines, had_signature) = header_part
            .split('\n')
            .fold(
                (Vec::new(), false, FoldState::Normal),
                |(mut lines, had_sig, state), line| match state {
                    FoldState::InSignature if line.starts_with(' ') || line.starts_with('\t') => {
                        (lines, had_sig, FoldState::InSignature)
                    }
                    _ if line.starts_with("gpgsig ") => (lines, true, FoldState::InSignature),
                    _ if line.starts_with("x-nonce ") => (lines, had_sig, FoldState::Normal),
                    _ => {
                        lines.push(line.to_string());
                        (lines, had_sig, FoldState::Normal)
                    }
                },
            )
            .into_tuple();

        Ok(CommitObject {
            header_lines,
            message: message.to_string(),
            had_signature,
        })
    }

    /// Reconstruct the commit content with a given nonce.
    /// Returns the raw content (without the "commit <len>\0" prefix).
    #[cfg(test)]
    pub fn build_with_nonce(&self, nonce: &[u8]) -> Vec<u8> {
        self.header_lines
            .iter()
            .flat_map(|line| line.as_bytes().iter().chain(std::iter::once(&b'\n')))
            .chain(b"x-nonce ".iter())
            .chain(nonce.iter())
            .chain(b"\n\n".iter())
            .chain(self.message.as_bytes().iter())
            .copied()
            .collect()
    }
}

/// Helper trait to destructure the fold accumulator.
trait IntoTuple {
    fn into_tuple(self) -> (Vec<String>, bool);
}

impl IntoTuple for (Vec<String>, bool, FoldState) {
    fn into_tuple(self) -> (Vec<String>, bool) {
        (self.0, self.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_commit() {
        let raw = "tree abc123\nparent def456\nauthor A <a@b> 1000 +0000\ncommitter B <b@c> 1000 +0000\n\nHello world\n";
        let commit = CommitObject::parse(raw).unwrap();
        assert_eq!(commit.header_lines.len(), 4);
        assert_eq!(commit.message, "Hello world\n");
        assert!(!commit.had_signature);
    }

    #[test]
    fn test_parse_with_signature() {
        let raw = "tree abc\nauthor A <a@b> 1 +0\ncommitter B <b@c> 1 +0\ngpgsig -----BEGIN PGP SIGNATURE-----\n abc\n -----END PGP SIGNATURE-----\n\nMsg\n";
        let commit = CommitObject::parse(raw).unwrap();
        assert!(commit.had_signature);
        // gpgsig lines should be stripped
        assert!(commit.header_lines.iter().all(|l| !l.starts_with("gpgsig")));
    }

    #[test]
    fn test_build_with_nonce() {
        let raw = "tree abc\nauthor A <a@b> 1 +0\ncommitter B <b@c> 1 +0\n\nMsg\n";
        let commit = CommitObject::parse(raw).unwrap();
        let nonce = vec![0x80, 0x81, 0x82];
        let content = commit.build_with_nonce(&nonce);
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("x-nonce "));
        assert!(content_str.contains("Msg"));
    }
}
