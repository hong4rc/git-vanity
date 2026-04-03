use regex::Regex;
use std::fmt;

/// Where in the hash to match the pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum MatchPosition {
    #[default]
    Start,
    Contains,
    End,
}

impl fmt::Display for MatchPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchPosition::Start => write!(f, "start"),
            MatchPosition::Contains => write!(f, "contains"),
            MatchPosition::End => write!(f, "end"),
        }
    }
}

impl MatchPosition {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "start" => Ok(MatchPosition::Start),
            "contains" | "include" => Ok(MatchPosition::Contains),
            "end" => Ok(MatchPosition::End),
            _ => Err(format!(
                "Invalid match position '{}': must be start, contains, or end",
                s
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Pattern {
    /// Match hex nibbles: `cafe`
    Prefix(Vec<u8>),
    /// Match N identical consecutive chars: `repeat:3` → aaa, 111, fff
    Repeat(usize),
    /// Structured: `1997xxx` → prefix "1997" + 3 identical chars
    Structured { prefix_nibbles: Vec<u8>, repeat_count: usize },
    /// Pair: `xx` → any 2 identical adjacent hex chars
    Pair,
    /// Regex: `/pattern/`
    RegexPattern(Regex),
}

/// Extract the nibble at position `i` from a 20-byte SHA-1 hash.
#[inline]
fn nibble_at(hash: &[u8; 20], i: usize) -> u8 {
    let byte = hash[i / 2];
    if i % 2 == 0 { (byte >> 4) & 0x0F } else { byte & 0x0F }
}

/// Parse a lowercase hex string into a Vec of nibble values (0-15).
fn hex_to_nibbles(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| c.to_digit(16).unwrap() as u8)
        .collect()
}

/// Convert nibbles back to a hex string for display.
fn nibbles_to_hex(nibbles: &[u8]) -> String {
    nibbles
        .iter()
        .map(|n| char::from_digit(*n as u32, 16).unwrap())
        .collect()
}

/// Check if `nibbles` match at position `offset` in the hash.
#[inline]
fn nibbles_match_at(hash: &[u8; 20], nibbles: &[u8], offset: usize) -> bool {
    offset + nibbles.len() <= 40
        && nibbles
            .iter()
            .enumerate()
            .all(|(i, &expected)| nibble_at(hash, offset + i) == expected)
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Pattern::Prefix(nibbles) => write!(f, "prefix(\"{}\")", nibbles_to_hex(nibbles)),
            Pattern::Repeat(n) => write!(f, "repeat({})", n),
            Pattern::Structured { prefix_nibbles, repeat_count } => {
                write!(f, "structured(\"{}\"+{}x)", nibbles_to_hex(prefix_nibbles), repeat_count)
            }
            Pattern::Pair => write!(f, "pair"),
            Pattern::RegexPattern(r) => write!(f, "regex(\"{}\")", r.as_str()),
        }
    }
}

impl Pattern {
    /// Parse using chain-of-responsibility: each parser returns Some if it handles the input.
    pub fn parse(input: &str, no_repeat: bool) -> Result<Self, String> {
        let parsers: &[fn(&str, bool) -> Option<Result<Pattern, String>>] = &[
            Self::parse_regex,
            Self::parse_repeat,
            Self::parse_pair,
            Self::parse_structured,
            Self::parse_prefix,
        ];

        parsers
            .iter()
            .find_map(|parser| parser(input, no_repeat))
            .unwrap_or_else(|| Err(format!("Invalid pattern '{}'", input)))
    }

    fn parse_regex(input: &str, _no_repeat: bool) -> Option<Result<Pattern, String>> {
        input
            .strip_prefix('/')
            .and_then(|s| s.strip_suffix('/'))
            .filter(|s| !s.is_empty())
            .map(|regex_str| {
                Regex::new(regex_str)
                    .map(Pattern::RegexPattern)
                    .map_err(|e| format!("Invalid regex: {}", e))
            })
    }

    fn parse_repeat(input: &str, _no_repeat: bool) -> Option<Result<Pattern, String>> {
        input.strip_prefix("repeat:").map(|n_str| {
            n_str
                .parse::<usize>()
                .map_err(|_| format!("Invalid repeat count: {}", n_str))
                .and_then(|n| {
                    if n < 2 {
                        Err("Repeat count must be >= 2".into())
                    } else {
                        Ok(Pattern::Repeat(n))
                    }
                })
        })
    }

    fn parse_pair(input: &str, _no_repeat: bool) -> Option<Result<Pattern, String>> {
        (input == "xx").then(|| Ok(Pattern::Pair))
    }

    fn parse_structured(input: &str, no_repeat: bool) -> Option<Result<Pattern, String>> {
        if no_repeat {
            return None;
        }

        let x_start = input.find('x')?;
        let (prefix_str, suffix) = input.split_at(x_start);

        let is_valid = !prefix_str.is_empty()
            && suffix.chars().all(|c| c == 'x')
            && prefix_str.chars().all(|c| c.is_ascii_hexdigit());

        is_valid.then(|| {
            Ok(Pattern::Structured {
                prefix_nibbles: hex_to_nibbles(&prefix_str.to_ascii_lowercase()),
                repeat_count: suffix.len(),
            })
        })
    }

    fn parse_prefix(input: &str, _no_repeat: bool) -> Option<Result<Pattern, String>> {
        let lower = input.to_ascii_lowercase();
        if !lower.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(Err(format!(
                "Invalid pattern '{}': prefix must be hex [0-9a-f]",
                input
            )));
        }
        Some(Ok(Pattern::Prefix(hex_to_nibbles(&lower))))
    }

    /// Check if a raw SHA-1 hash (20 bytes) matches this pattern at the given position.
    pub fn matches_raw(&self, hash: &[u8; 20], position: MatchPosition) -> bool {
        match self {
            Pattern::Prefix(nibbles) => match position {
                MatchPosition::Start => nibbles_match_at(hash, nibbles, 0),
                MatchPosition::End => nibbles_match_at(hash, nibbles, 40 - nibbles.len()),
                MatchPosition::Contains => {
                    (0..=40 - nibbles.len()).any(|offset| nibbles_match_at(hash, nibbles, offset))
                }
            },

            Pattern::Repeat(n) => {
                let mut run = 1usize;
                let mut prev = nibble_at(hash, 0);
                for i in 1..40 {
                    let curr = nibble_at(hash, i);
                    if curr == prev {
                        run += 1;
                        if run >= *n {
                            return true;
                        }
                    } else {
                        run = 1;
                        prev = curr;
                    }
                }
                false
            }

            Pattern::Structured { prefix_nibbles, repeat_count } => match position {
                MatchPosition::Start => {
                    nibbles_match_at(hash, prefix_nibbles, 0) && {
                        let start = prefix_nibbles.len();
                        start + *repeat_count <= 40 && {
                            let first = nibble_at(hash, start);
                            (1..*repeat_count).all(|i| nibble_at(hash, start + i) == first)
                        }
                    }
                }
                MatchPosition::End | MatchPosition::Contains => {
                    let total_len = prefix_nibbles.len() + *repeat_count;
                    let mut range = match position {
                        MatchPosition::End => (40 - total_len)..=(40 - total_len),
                        _ => 0..=(40 - total_len),
                    };
                    range.any(|offset| {
                        nibbles_match_at(hash, prefix_nibbles, offset) && {
                            let start = offset + prefix_nibbles.len();
                            let first = nibble_at(hash, start);
                            (1..*repeat_count).all(|i| nibble_at(hash, start + i) == first)
                        }
                    })
                }
            },

            // Pair: check adjacent nibbles without allocation
            Pattern::Pair => {
                // Check within-byte pairs (high == low nibble) first
                // Then cross-byte pairs (low nibble of byte N == high nibble of byte N+1)
                (0..20).any(|i| {
                    let b = hash[i];
                    let hi = (b >> 4) & 0x0F;
                    let lo = b & 0x0F;
                    // Within-byte pair
                    if hi == lo { return true; }
                    // Cross-byte pair (low of this byte == high of next)
                    if i < 19 {
                        let next_hi = (hash[i + 1] >> 4) & 0x0F;
                        if lo == next_hi { return true; }
                    }
                    false
                })
            }

            Pattern::RegexPattern(re) => re.is_match(&hex::encode(hash)),
        }
    }

    /// Estimated difficulty (number of expected attempts)
    pub fn estimated_attempts(&self, position: MatchPosition) -> u64 {
        let base = match self {
            Pattern::Prefix(nibbles) => 16u64.pow(nibbles.len() as u32),
            Pattern::Repeat(n) => 16u64.pow(*n as u32) / 16,
            Pattern::Structured { prefix_nibbles, repeat_count } => {
                16u64.pow(prefix_nibbles.len() as u32) * 16u64.pow(*repeat_count as u32) / 16
            }
            Pattern::Pair => 16,
            Pattern::RegexPattern(_) => 1_000_000,
        };

        // Contains/End are easier for Prefix/Structured because there are more
        // positions to match. Rough approximation: divide by number of positions.
        match (self, position) {
            (Pattern::Prefix(n), MatchPosition::Contains) => base / (41 - n.len() as u64),
            (Pattern::Prefix(_), MatchPosition::End) => base,
            (Pattern::Structured { prefix_nibbles, repeat_count, .. }, MatchPosition::Contains) => {
                base / (41 - (prefix_nibbles.len() + repeat_count) as u64)
            }
            _ => base,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_parse() {
        let p = Pattern::parse("cafe", false).unwrap();
        assert!(matches!(p, Pattern::Prefix(_)));
    }

    #[test]
    fn test_repeat_parse() {
        let p = Pattern::parse("repeat:3", false).unwrap();
        assert!(matches!(p, Pattern::Repeat(3)));
    }

    #[test]
    fn test_pair_parse() {
        let p = Pattern::parse("xx", false).unwrap();
        assert!(matches!(p, Pattern::Pair));
    }

    #[test]
    fn test_prefix_match_start() {
        let p = Pattern::parse("00", false).unwrap();
        let mut hash = [0u8; 20];
        hash[0] = 0x00;
        assert!(p.matches_raw(&hash, MatchPosition::Start));

        hash[0] = 0x01;
        assert!(!p.matches_raw(&hash, MatchPosition::Start));
    }

    #[test]
    fn test_prefix_match_end() {
        let p = Pattern::parse("ff", false).unwrap();
        let mut hash = [0u8; 20];
        hash[19] = 0xFF;
        assert!(p.matches_raw(&hash, MatchPosition::End));

        hash[19] = 0x00;
        assert!(!p.matches_raw(&hash, MatchPosition::End));
    }

    #[test]
    fn test_prefix_match_contains() {
        let p = Pattern::parse("ab", false).unwrap();
        let mut hash = [0u8; 20];
        // Put "ab" in the middle
        hash[5] = 0xab;
        assert!(p.matches_raw(&hash, MatchPosition::Contains));

        // Also matches at start
        hash = [0u8; 20];
        hash[0] = 0xab;
        assert!(p.matches_raw(&hash, MatchPosition::Contains));
    }

    #[test]
    fn test_pair_match() {
        let p = Pattern::parse("xx", false).unwrap();
        let mut hash = [0u8; 20];
        assert!(p.matches_raw(&hash, MatchPosition::Start));

        hash = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78];
        assert!(!p.matches_raw(&hash, MatchPosition::Start));
    }

    #[test]
    fn test_structured_odd_prefix() {
        let p = Pattern::parse("99999x", false).unwrap();
        assert!(matches!(p, Pattern::Structured { .. }));
        assert_eq!(format!("{}", p), "structured(\"99999\"+1x)");
    }

    #[test]
    fn test_invalid_pattern() {
        assert!(Pattern::parse("xyz", false).is_err());
    }

    #[test]
    fn test_repeat_count_too_small() {
        assert!(Pattern::parse("repeat:1", false).is_err());
        assert!(Pattern::parse("repeat:0", false).is_err());
    }

    #[test]
    fn test_repeat_invalid_number() {
        assert!(Pattern::parse("repeat:abc", false).is_err());
    }

    #[test]
    fn test_repeat_match() {
        let p = Pattern::parse("repeat:3", false).unwrap();
        let hash = [0x11; 20];
        assert!(p.matches_raw(&hash, MatchPosition::Start));

        let hash2 = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34,
                0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78];
        assert!(!p.matches_raw(&hash2, MatchPosition::Start));
    }

    #[test]
    fn test_regex_match() {
        let p = Pattern::parse("/^dead/", false).unwrap();
        let mut hash = [0u8; 20];
        hash[0] = 0xde;
        hash[1] = 0xad;
        assert!(p.matches_raw(&hash, MatchPosition::Start));

        hash[0] = 0x00;
        assert!(!p.matches_raw(&hash, MatchPosition::Start));
    }

    #[test]
    fn test_structured_match() {
        let p = Pattern::parse("aaxxx", false).unwrap();
        let mut hash = [0u8; 20];
        hash[0] = 0xaa;
        hash[1] = 0x11;
        hash[2] = 0x10;
        assert!(p.matches_raw(&hash, MatchPosition::Start));

        hash[0] = 0xbb;
        assert!(!p.matches_raw(&hash, MatchPosition::Start));
    }

    #[test]
    fn test_structured_match_end() {
        let p = Pattern::parse("aax", false).unwrap();
        let mut hash = [0u8; 20];
        // "aa" + 1 identical at the end: nibbles 37,38,39 = a,a,X
        // hash[18] high=a, hash[18] low=a, hash[19] high=X
        hash[18] = 0xaa;
        hash[19] = 0x50; // nibble 38=a, 39=5... no that's wrong
        // Let's think: end means the structured pattern occupies the last 3 nibbles
        // nibbles 37=a, 38=a, 39=X (X=any identical, just 1 so always matches)
        // hash[18] = byte containing nibbles 36,37 → nibble 37 = low nibble = 0x0a
        // hash[19] = byte containing nibbles 38,39 → nibble 38 = high = 0x0a, nibble 39 = low
        hash[18] = 0x0a; // nibble 36=0, 37=a
        hash[19] = 0xa0; // nibble 38=a, 39=0 → repeat char = 0
        assert!(p.matches_raw(&hash, MatchPosition::End));
    }

    #[test]
    fn test_structured_match_contains() {
        let p = Pattern::parse("aax", false).unwrap();
        let mut hash = [0u8; 20];
        hash[5] = 0xaa;
        hash[6] = 0x30; // nibbles: ...,a,a,3,0,...  repeat=3 (1 char, always matches)
        assert!(p.matches_raw(&hash, MatchPosition::Contains));
    }

    #[test]
    fn test_display_all_variants() {
        assert_eq!(format!("{}", Pattern::parse("cafe", false).unwrap()), "prefix(\"cafe\")");
        assert_eq!(format!("{}", Pattern::parse("repeat:3", false).unwrap()), "repeat(3)");
        assert_eq!(format!("{}", Pattern::parse("xx", false).unwrap()), "pair");
        assert!(format!("{}", Pattern::parse("/^dead/", false).unwrap()).contains("regex"));
        assert!(format!("{}", Pattern::parse("aaxxx", false).unwrap()).contains("structured"));
    }

    #[test]
    fn test_estimated_attempts_all() {
        assert_eq!(Pattern::parse("cafe", false).unwrap().estimated_attempts(MatchPosition::Start), 65536);
        assert_eq!(Pattern::parse("repeat:3", false).unwrap().estimated_attempts(MatchPosition::Start), 256);
        assert_eq!(Pattern::parse("xx", false).unwrap().estimated_attempts(MatchPosition::Start), 16);
        assert_eq!(Pattern::parse("/^dead/", false).unwrap().estimated_attempts(MatchPosition::Start), 1_000_000);
        assert!(Pattern::parse("aaxxx", false).unwrap().estimated_attempts(MatchPosition::Start) > 0);
    }

    #[test]
    fn test_estimated_contains_easier() {
        let p = Pattern::parse("cafe", false).unwrap();
        assert!(p.estimated_attempts(MatchPosition::Contains) < p.estimated_attempts(MatchPosition::Start));
    }

    #[test]
    fn test_regex_invalid() {
        assert!(Pattern::parse("/[invalid/", false).is_err());
    }

    #[test]
    fn test_match_position_parse() {
        assert_eq!(MatchPosition::parse("start").unwrap(), MatchPosition::Start);
        assert_eq!(MatchPosition::parse("contains").unwrap(), MatchPosition::Contains);
        assert_eq!(MatchPosition::parse("include").unwrap(), MatchPosition::Contains);
        assert_eq!(MatchPosition::parse("end").unwrap(), MatchPosition::End);
        assert!(MatchPosition::parse("middle").is_err());
    }

    #[test]
    fn test_match_position_display() {
        assert_eq!(format!("{}", MatchPosition::Start), "start");
        assert_eq!(format!("{}", MatchPosition::Contains), "contains");
        assert_eq!(format!("{}", MatchPosition::End), "end");
    }
}
