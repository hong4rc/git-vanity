/// Curated hex word presets — meaningful words using only 0-9 and a-f.
///
/// Organized by difficulty (prefix length → search time).

#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub name: &'static str,
    pub hex: &'static str,
    pub description: &'static str,
}

/// All available presets, sorted by difficulty (easiest first).
pub const PRESETS: &[Preset] = &[
    // 3-char — instant
    Preset { name: "ace",      hex: "ace",      description: "an ace up your sleeve" },
    Preset { name: "add",      hex: "add",      description: "add something new" },
    Preset { name: "bad",      hex: "bad",      description: "bad to the bone" },
    Preset { name: "bed",      hex: "bed",      description: "time for bed" },
    Preset { name: "cab",      hex: "cab",      description: "hail a cab" },
    Preset { name: "dad",      hex: "dad",      description: "hi dad" },
    Preset { name: "fab",      hex: "fab",      description: "fabulous" },
    Preset { name: "fed",      hex: "fed",      description: "well fed" },
    // 4-char — < 1s
    Preset { name: "babe",     hex: "babe",     description: "hey babe" },
    Preset { name: "bead",     hex: "bead",     description: "string of beads" },
    Preset { name: "beef",     hex: "beef",     description: "where's the beef?" },
    Preset { name: "cafe",     hex: "cafe",     description: "coffee shop vibes" },
    Preset { name: "code",     hex: "c0de",     description: "write some c0de" },
    Preset { name: "daze",     hex: "da2e",     description: "in a da2e" },
    Preset { name: "dead",     hex: "dead",     description: "dead commit walking" },
    Preset { name: "deaf",     hex: "deaf",     description: "deaf to criticism" },
    Preset { name: "face",     hex: "face",     description: "save face" },
    Preset { name: "fade",     hex: "fade",     description: "fade to black" },
    Preset { name: "feed",     hex: "feed",     description: "feed the code" },
    Preset { name: "food",     hex: "f00d",     description: "f00d for thought" },
    // 5-char — ~ 1-2s
    Preset { name: "decaf",    hex: "decaf",    description: "decaf coffee" },
    // 6-char — ~ 5s
    Preset { name: "coffee",   hex: "c0ffee",   description: "powered by c0ffee" },
    Preset { name: "decade",   hex: "decade",   description: "a decade of commits" },
    Preset { name: "deface",   hex: "deface",   description: "deface the hash" },
    Preset { name: "facade",   hex: "facade",   description: "behind the facade" },
    // 7-char — ~ 30s+
    Preset { name: "defaced",  hex: "defaced",  description: "defaced and proud" },
    Preset { name: "effaced",  hex: "effaced",  description: "effaced from history" },
];

/// Find a preset by name (case-insensitive).
pub fn find(name: &str) -> Option<&'static Preset> {
    let lower = name.to_ascii_lowercase();
    PRESETS.iter().find(|p| p.name == lower)
}

/// Format all presets for display.
pub fn list() -> String {
    let max_name = PRESETS.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let max_hex = PRESETS.iter().map(|p| p.hex.len()).max().unwrap_or(0);

    PRESETS
        .iter()
        .map(|p| {
            let difficulty = match p.hex.len() {
                0..=3 => "instant",
                4     => "< 1s",
                5     => "~ 2s",
                6     => "~ 5s",
                _     => "~ 30s+",
            };
            format!(
                "  {:<width_n$}  {:<width_h$}  {:>8}  {}",
                p.name,
                p.hex,
                difficulty,
                p.description,
                width_n = max_name,
                width_h = max_hex,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_existing() {
        assert_eq!(find("cafe").unwrap().hex, "cafe");
        assert_eq!(find("coffee").unwrap().hex, "c0ffee");
        assert_eq!(find("dead").unwrap().hex, "dead");
    }

    #[test]
    fn test_find_case_insensitive() {
        assert!(find("CAFE").is_some());
        assert!(find("Cafe").is_some());
    }

    #[test]
    fn test_find_nonexistent() {
        assert!(find("banana").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn test_all_presets_are_valid_hex() {
        for p in PRESETS {
            assert!(
                p.hex.chars().all(|c| c.is_ascii_hexdigit()),
                "Preset '{}' has invalid hex: {}",
                p.name,
                p.hex
            );
        }
    }

    #[test]
    fn test_list_contains_all_presets() {
        let output = list();
        for p in PRESETS {
            assert!(output.contains(p.name), "Missing preset: {}", p.name);
        }
    }
}
