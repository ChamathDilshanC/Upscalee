//! Quality presets. "K" is defined by the **longest edge** of the output so
//! portrait and landscape sources behave the same way.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Preset {
    #[serde(rename = "2K")]
    K2,
    #[serde(rename = "4K")]
    K4,
    #[serde(rename = "8K")]
    K8,
    #[serde(rename = "12K")]
    K12,
    #[serde(rename = "16K")]
    K16,
}

impl Preset {
    /// All presets, smallest first.
    pub const ALL: [Preset; 5] = [Preset::K2, Preset::K4, Preset::K8, Preset::K12, Preset::K16];

    /// Longest edge of the output image in pixels.
    pub const fn long_edge(self) -> u32 {
        match self {
            Preset::K2 => 2048,
            Preset::K4 => 3840,
            Preset::K8 => 7680,
            Preset::K12 => 11520,
            Preset::K16 => 15360,
        }
    }

    /// Label used in the UI and in output file names (`photo_4K.png`).
    pub const fn label(self) -> &'static str {
        match self {
            Preset::K2 => "2K",
            Preset::K4 => "4K",
            Preset::K8 => "8K",
            Preset::K12 => "12K",
            Preset::K16 => "16K",
        }
    }

    /// Keyboard shortcut in the radial menu.
    pub const fn hotkey(self) -> char {
        match self {
            Preset::K2 => '2',
            Preset::K4 => '4',
            Preset::K8 => '8',
            Preset::K12 => '1',
            Preset::K16 => '6',
        }
    }

    /// The next smaller preset, if any.
    pub fn smaller(self) -> Option<Preset> {
        let idx = Preset::ALL.iter().position(|p| *p == self)?;
        idx.checked_sub(1).map(|i| Preset::ALL[i])
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown preset '{0}' (expected 2K, 4K, 8K, 12K or 16K)")]
pub struct UnknownPreset(pub String);

impl FromStr for Preset {
    type Err = UnknownPreset;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        let digits = trimmed.trim_end_matches(['k', 'K']);
        match digits {
            "2" => Ok(Preset::K2),
            "4" => Ok(Preset::K4),
            "8" => Ok(Preset::K8),
            "12" => Ok(Preset::K12),
            "16" => Ok(Preset::K16),
            _ => Err(UnknownPreset(trimmed.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_edges_match_the_spec() {
        let edges: Vec<u32> = Preset::ALL.iter().map(|p| p.long_edge()).collect();
        assert_eq!(edges, [2048, 3840, 7680, 11520, 15360]);
    }

    #[test]
    fn parses_labels_case_insensitively() {
        assert_eq!("4k".parse::<Preset>().unwrap(), Preset::K4);
        assert_eq!(" 16K ".parse::<Preset>().unwrap(), Preset::K16);
        assert_eq!("12".parse::<Preset>().unwrap(), Preset::K12);
        assert!("5K".parse::<Preset>().is_err());
    }

    #[test]
    fn serde_uses_labels() {
        assert_eq!(serde_json::to_string(&Preset::K12).unwrap(), "\"12K\"");
        assert_eq!(serde_json::from_str::<Preset>("\"2K\"").unwrap(), Preset::K2);
    }

    #[test]
    fn smaller_walks_down() {
        assert_eq!(Preset::K8.smaller(), Some(Preset::K4));
        assert_eq!(Preset::K2.smaller(), None);
    }

    #[test]
    fn hotkeys_are_unique() {
        let mut keys: Vec<char> = Preset::ALL.iter().map(|p| p.hotkey()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Preset::ALL.len());
    }
}
