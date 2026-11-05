//! Output file naming: `photo.png` + 4K → `photo_4K.png`, then
//! `photo_4K (1).png`, `photo_4K (2).png`, … when the name is taken.

use std::path::{Path, PathBuf};

use crate::preset::Preset;

/// Upper bound on collision suffixes before falling back to a random suffix.
const MAX_SUFFIX: u32 = 9_999;

pub fn output_file_name(stem: &str, preset: Preset, ext: &str, n: u32) -> String {
    if n == 0 {
        format!("{stem}_{}.{ext}", preset.label())
    } else {
        format!("{stem}_{} ({n}).{ext}", preset.label())
    }
}

/// File stem of `source` (lossy for non-UTF-8 names).
pub fn source_stem(source: &Path) -> String {
    source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "image".to_owned())
}

/// Directory the output should go to: the configured folder, or next to the source.
pub fn output_dir(source: &Path, configured: Option<&Path>) -> PathBuf {
    match configured {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => source.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
    }
}

/// First free path in `dir` according to `is_taken`.
pub fn unique_output_path(
    dir: &Path,
    stem: &str,
    preset: Preset,
    ext: &str,
    is_taken: impl Fn(&Path) -> bool,
) -> PathBuf {
    for n in 0..=MAX_SUFFIX {
        let candidate = dir.join(output_file_name(stem, preset, ext, n));
        if !is_taken(&candidate) {
            return candidate;
        }
    }
    let random = uuid::Uuid::new_v4().simple().to_string();
    dir.join(format!("{stem}_{} {}.{ext}", preset.label(), &random[..8]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_follow_the_spec() {
        assert_eq!(output_file_name("photo", Preset::K4, "png", 0), "photo_4K.png");
        assert_eq!(output_file_name("photo", Preset::K4, "png", 2), "photo_4K (2).png");
        assert_eq!(output_file_name("Confident Portrait", Preset::K8, "jpg", 0), "Confident Portrait_8K.jpg");
    }

    #[test]
    fn collisions_get_numbered_suffixes() {
        let dir = Path::new(r"C:\pics");
        let taken: HashSet<PathBuf> = [dir.join("photo_4K.png"), dir.join("photo_4K (1).png")].into();
        let p = unique_output_path(dir, "photo", Preset::K4, "png", |p| taken.contains(p));
        assert_eq!(p, dir.join("photo_4K (2).png"));
    }

    #[test]
    fn output_dir_defaults_to_source_folder() {
        let src = Path::new(r"C:\pics\a.png");
        assert_eq!(output_dir(src, None), Path::new(r"C:\pics"));
        assert_eq!(output_dir(src, Some(Path::new(""))), Path::new(r"C:\pics"));
        assert_eq!(output_dir(src, Some(Path::new(r"D:\out"))), Path::new(r"D:\out"));
    }

    #[test]
    fn stems_handle_odd_names() {
        assert_eq!(source_stem(Path::new(r"C:\a\my.photo.final.png")), "my.photo.final");
        assert_eq!(source_stem(Path::new(r"C:\a\.png")), ".png");
    }
}
