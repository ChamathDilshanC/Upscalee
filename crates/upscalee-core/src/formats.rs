//! Source formats, output formats and AI model profiles.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Formats Upscalee can read (by file extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    Png,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
}

impl SourceFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "bmp" => Some(Self::Bmp),
            "tif" | "tiff" => Some(Self::Tiff),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension().and_then(|e| e.to_str()).and_then(Self::from_extension)
    }

    /// Short upper-case label for the UI (`1 × PNG`).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPG",
            Self::Webp => "WEBP",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
        }
    }
}

/// User-facing output format choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Same as the source (BMP and TIFF become PNG).
    #[default]
    Keep,
    Png,
    Jpg,
    Webp,
}

impl OutputFormat {
    pub fn resolve(self, source: SourceFormat) -> EncodedFormat {
        match self {
            OutputFormat::Png => EncodedFormat::Png,
            OutputFormat::Jpg => EncodedFormat::Jpeg,
            OutputFormat::Webp => EncodedFormat::Webp,
            OutputFormat::Keep => match source {
                SourceFormat::Jpeg => EncodedFormat::Jpeg,
                SourceFormat::Webp => EncodedFormat::Webp,
                SourceFormat::Png | SourceFormat::Bmp | SourceFormat::Tiff => EncodedFormat::Png,
            },
        }
    }
}

/// The concrete encoder used for an output file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodedFormat {
    Png,
    Jpeg,
    Webp,
}

impl EncodedFormat {
    pub const fn default_extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    /// Largest width/height the format can store.
    pub const fn max_dimension(self) -> u32 {
        match self {
            Self::Png => i32::MAX as u32,
            Self::Jpeg => 65_535,
            Self::Webp => 16_383,
        }
    }

    pub const fn supports_alpha(self) -> bool {
        !matches!(self, Self::Jpeg)
    }
}

/// Extension to use for the output file. When the encoder matches the source
/// we keep the user's spelling (`.jpeg` stays `.jpeg`).
pub fn output_extension(source: &Path, encoded: EncodedFormat) -> String {
    let source_ext = source.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    match (encoded, source_ext.as_deref()) {
        (EncodedFormat::Jpeg, Some(ext @ ("jpg" | "jpeg"))) => ext.to_owned(),
        _ => encoded.default_extension().to_owned(),
    }
}

/// AI model profile, cycled with Tab in the radial menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelProfile {
    /// General photos: `realesrgan-x4plus`.
    #[default]
    Photo,
    /// Illustrations, anime, line art: `realesrgan-x4plus-anime`.
    Anime,
}

impl ModelProfile {
    /// Every bundled model upscales by exactly this factor.
    pub const MODEL_SCALE: u32 = 4;

    pub const fn model_name(self) -> &'static str {
        match self {
            Self::Photo => "realesrgan-x4plus",
            Self::Anime => "realesrgan-x4plus-anime",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Photo => "Photo",
            Self::Anime => "Anime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_source_formats() {
        assert_eq!(SourceFormat::from_path(Path::new("a/B.JPEG")), Some(SourceFormat::Jpeg));
        assert_eq!(SourceFormat::from_path(Path::new("a.tif")), Some(SourceFormat::Tiff));
        assert_eq!(SourceFormat::from_path(Path::new("a.gif")), None);
        assert_eq!(SourceFormat::from_path(Path::new("noext")), None);
    }

    #[test]
    fn keep_maps_lossless_containers_to_png() {
        assert_eq!(OutputFormat::Keep.resolve(SourceFormat::Bmp), EncodedFormat::Png);
        assert_eq!(OutputFormat::Keep.resolve(SourceFormat::Tiff), EncodedFormat::Png);
        assert_eq!(OutputFormat::Keep.resolve(SourceFormat::Jpeg), EncodedFormat::Jpeg);
        assert_eq!(OutputFormat::Keep.resolve(SourceFormat::Webp), EncodedFormat::Webp);
        assert_eq!(OutputFormat::Webp.resolve(SourceFormat::Png), EncodedFormat::Webp);
    }

    #[test]
    fn output_extension_preserves_jpeg_spelling() {
        assert_eq!(output_extension(Path::new("x.JPEG"), EncodedFormat::Jpeg), "jpeg");
        assert_eq!(output_extension(Path::new("x.jpg"), EncodedFormat::Jpeg), "jpg");
        assert_eq!(output_extension(Path::new("x.png"), EncodedFormat::Jpeg), "jpg");
        assert_eq!(output_extension(Path::new("x.tiff"), EncodedFormat::Png), "png");
    }

    #[test]
    fn sixteen_k_fits_every_encoder() {
        for f in [EncodedFormat::Png, EncodedFormat::Jpeg, EncodedFormat::Webp] {
            assert!(f.max_dimension() >= crate::Preset::K16.long_edge());
        }
    }

    #[test]
    fn extension_lists_agree_with_ipc() {
        for ext in upscalee_ipc::SUPPORTED_EXTENSIONS {
            assert!(SourceFormat::from_extension(ext).is_some(), "{ext}");
        }
    }
}
