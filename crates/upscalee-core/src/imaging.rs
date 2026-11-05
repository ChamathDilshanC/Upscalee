//! Image decode / resize / encode and atomic file writes.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::metadata::Orientation;
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, RgbImage, RgbaImage};

use crate::formats::{EncodedFormat, SourceFormat};
use crate::planner::Dimensions;

pub const JPEG_QUALITY: u8 = 95;
pub const WEBP_QUALITY: f32 = 95.0;

#[derive(Debug, thiserror::Error)]
pub enum ImagingError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("could not read image: {0}")]
    Decode(String),
    #[error("could not encode image: {0}")]
    Encode(String),
    #[error("{format:?} cannot store images larger than {max} px per side")]
    TooLarge { format: EncodedFormat, max: u32 },
    #[error("image is empty")]
    Empty,
}

impl From<image::ImageError> for ImagingError {
    fn from(err: image::ImageError) -> Self {
        match err {
            image::ImageError::IoError(io) => ImagingError::Io(io),
            other => ImagingError::Decode(other.to_string()),
        }
    }
}

/// Header-level information, cheap to obtain (no pixel decode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    /// Display dimensions (EXIF orientation applied).
    pub dimensions: Dimensions,
    pub format: SourceFormat,
}

fn open_reader(path: &Path) -> Result<ImageReader<std::io::BufReader<File>>, ImagingError> {
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    // Large AI outputs (16K ≈ 0.5 GB) exceed the crate's default 512 MB limit.
    reader.no_limits();
    Ok(reader)
}

fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    )
}

/// Read dimensions (orientation-aware) without decoding pixels.
pub fn read_info(path: &Path) -> Result<ImageInfo, ImagingError> {
    let format = SourceFormat::from_path(path).ok_or_else(|| ImagingError::Decode("unsupported file type".into()))?;
    let mut decoder = open_reader(path)?.into_decoder()?;
    let (w, h) = decoder.dimensions();
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let dimensions = if swaps_axes(orientation) { Dimensions::new(h, w) } else { Dimensions::new(w, h) };
    if dimensions.is_empty() {
        return Err(ImagingError::Empty);
    }
    Ok(ImageInfo { dimensions, format })
}

/// A decoded image normalised to 8-bit RGB (or RGBA when it has real transparency).
pub struct DecodedImage {
    pub image: DynamicImage,
    pub icc_profile: Option<Vec<u8>>,
}

impl DecodedImage {
    pub fn dimensions(&self) -> Dimensions {
        Dimensions::new(self.image.width(), self.image.height())
    }

    pub fn channels(&self) -> u32 {
        self.image.color().channel_count() as u32
    }
}

/// Decode, apply EXIF orientation, and normalise the pixel format.
pub fn decode(path: &Path) -> Result<DecodedImage, ImagingError> {
    let mut decoder = open_reader(path)?.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let icc_profile = decoder.icc_profile().ok().flatten();
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    if image.width() == 0 || image.height() == 0 {
        return Err(ImagingError::Empty);
    }
    Ok(DecodedImage { image: normalize(image), icc_profile })
}

/// 8-bit RGB, or RGBA only if some pixel is actually translucent. Dropping an
/// all-opaque alpha channel makes the AI pass and every later stage cheaper.
pub fn normalize(image: DynamicImage) -> DynamicImage {
    match image {
        DynamicImage::ImageRgb8(_) => image,
        DynamicImage::ImageRgba8(rgba) => drop_opaque_alpha(rgba),
        other if other.color().has_alpha() => drop_opaque_alpha(other.into_rgba8()),
        other => DynamicImage::ImageRgb8(other.into_rgb8()),
    }
}

fn drop_opaque_alpha(rgba: RgbaImage) -> DynamicImage {
    if rgba.pixels().all(|p| p.0[3] == u8::MAX) {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).into_rgb8())
    } else {
        DynamicImage::ImageRgba8(rgba)
    }
}

/// Exact Lanczos3 resize (SIMD + multi-threaded, alpha-correct).
pub fn resize(image: &DynamicImage, to: Dimensions) -> Result<DynamicImage, ImagingError> {
    if to.is_empty() {
        return Err(ImagingError::Empty);
    }
    if image.width() == to.width && image.height() == to.height {
        return Ok(image.clone());
    }
    let has_alpha = image.color().has_alpha();
    let source = if matches!(image, DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_)) {
        std::borrow::Cow::Borrowed(image)
    } else {
        std::borrow::Cow::Owned(normalize(image.clone()))
    };
    let pixel_type = if has_alpha { PixelType::U8x4 } else { PixelType::U8x3 };
    let mut dst = FirImage::new(to.width, to.height, pixel_type);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(source.as_ref(), &mut dst, &options)
        .map_err(|e| ImagingError::Encode(format!("resize failed: {e}")))?;
    let buffer = dst.into_vec();
    let out = if has_alpha {
        RgbaImage::from_raw(to.width, to.height, buffer).map(DynamicImage::ImageRgba8)
    } else {
        RgbImage::from_raw(to.width, to.height, buffer).map(DynamicImage::ImageRgb8)
    };
    out.ok_or_else(|| ImagingError::Encode("resize produced a buffer of unexpected size".into()))
}

/// Composite translucent pixels over white (for JPEG, which has no alpha).
pub fn flatten_on_white(image: &DynamicImage) -> RgbImage {
    match image {
        DynamicImage::ImageRgb8(rgb) => rgb.clone(),
        other => {
            let rgba = other.to_rgba8();
            let mut rgb = RgbImage::new(rgba.width(), rgba.height());
            for (dst, src) in rgb.pixels_mut().zip(rgba.pixels()) {
                let a = src.0[3] as u32;
                for c in 0..3 {
                    dst.0[c] = ((src.0[c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
                }
            }
            rgb
        }
    }
}

fn color_type(image: &DynamicImage) -> (ExtendedColorType, &[u8]) {
    match image {
        DynamicImage::ImageRgba8(rgba) => (ExtendedColorType::Rgba8, rgba.as_raw()),
        DynamicImage::ImageRgb8(rgb) => (ExtendedColorType::Rgb8, rgb.as_raw()),
        _ => unreachable!("images are normalised to RGB8/RGBA8 before encoding"),
    }
}

/// Encode a normalised image into `writer`.
pub fn encode<W: Write>(
    image: &DynamicImage,
    format: EncodedFormat,
    icc_profile: Option<&[u8]>,
    writer: W,
) -> Result<(), ImagingError> {
    let max = format.max_dimension();
    if image.width() > max || image.height() > max {
        return Err(ImagingError::TooLarge { format, max });
    }
    let normalized;
    let image = if matches!(image, DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_)) {
        image
    } else {
        normalized = normalize(image.clone());
        &normalized
    };
    match format {
        EncodedFormat::Png => encode_png(image, icc_profile, CompressionType::Default, writer),
        EncodedFormat::Jpeg => encode_jpeg(image, icc_profile, writer),
        EncodedFormat::Webp => encode_webp(image, writer),
    }
}

fn encode_png<W: Write>(
    image: &DynamicImage,
    icc_profile: Option<&[u8]>,
    compression: CompressionType,
    writer: W,
) -> Result<(), ImagingError> {
    let mut encoder = PngEncoder::new_with_quality(writer, compression, PngFilter::Adaptive);
    if let Some(icc) = icc_profile {
        // Best effort: an encoder that can't embed ICC still writes valid pixels.
        let _ = encoder.set_icc_profile(icc.to_vec());
    }
    let (color, bytes) = color_type(image);
    encoder.write_image(bytes, image.width(), image.height(), color).map_err(|e| ImagingError::Encode(e.to_string()))
}

fn encode_jpeg<W: Write>(image: &DynamicImage, icc_profile: Option<&[u8]>, writer: W) -> Result<(), ImagingError> {
    let rgb = flatten_on_white(image);
    let mut encoder = jpeg_encoder::Encoder::new(writer, JPEG_QUALITY);
    // 4:4:4 – no chroma subsampling; upscaled line art and text stay crisp.
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    if let Some(icc) = icc_profile {
        let _ = encoder.add_icc_profile(icc);
    }
    encoder
        .encode(rgb.as_raw(), rgb.width() as u16, rgb.height() as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| ImagingError::Encode(e.to_string()))
}

fn encode_webp<W: Write>(image: &DynamicImage, mut writer: W) -> Result<(), ImagingError> {
    let (w, h) = (image.width(), image.height());
    let encoder = match image {
        DynamicImage::ImageRgba8(rgba) => webp::Encoder::from_rgba(rgba.as_raw(), w, h),
        DynamicImage::ImageRgb8(rgb) => webp::Encoder::from_rgb(rgb.as_raw(), w, h),
        _ => unreachable!("normalised"),
    };
    let mut config = webp::WebPConfig::new().map_err(|_| ImagingError::Encode("libwebp init failed".into()))?;
    config.quality = WEBP_QUALITY;
    config.method = 4;
    config.thread_level = 1;
    let memory = encoder.encode_advanced(&config).map_err(|e| ImagingError::Encode(format!("{e:?}")))?;
    writer.write_all(&memory)?;
    Ok(())
}

/// Fast, lossless PNG for handing intermediates to the engine.
pub fn write_intermediate_png(image: &DynamicImage, path: &Path) -> Result<(), ImagingError> {
    let file = File::create(path)?;
    let mut writer = BufWriter::with_capacity(1 << 20, file);
    encode_png(image, None, CompressionType::Fast, &mut writer)?;
    writer.flush()?;
    Ok(())
}

/// Encode into a hidden temp file in `dir`, then move it into place without
/// ever overwriting an existing file. `next_path` is called again whenever the
/// chosen name turns out to be taken at the last moment.
pub fn save_atomic(
    image: &DynamicImage,
    format: EncodedFormat,
    icc_profile: Option<&[u8]>,
    dir: &Path,
    mut next_path: impl FnMut() -> PathBuf,
) -> Result<PathBuf, ImagingError> {
    let mut tmp = tempfile::Builder::new().prefix(".upscalee-").suffix(".tmp").tempfile_in(dir)?;
    {
        let mut writer = BufWriter::with_capacity(4 << 20, tmp.as_file_mut());
        encode(image, format, icc_profile, &mut writer)?;
        writer.flush()?;
    }
    tmp.as_file().sync_all()?;

    for _ in 0..64 {
        let target = next_path();
        match tmp.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(err) if err.error.kind() == std::io::ErrorKind::AlreadyExists => tmp = err.file,
            Err(err) => return Err(ImagingError::Io(err.error)),
        }
    }
    Err(ImagingError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not find a free output file name",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, Rgba};

    fn gradient(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| Rgb([(x * 7) as u8, (y * 5) as u8, 128])))
    }

    #[test]
    fn opaque_alpha_is_dropped_but_real_alpha_kept() {
        let opaque = DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255])));
        assert!(matches!(normalize(opaque), DynamicImage::ImageRgb8(_)));
        let mut translucent = RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255]));
        translucent.put_pixel(0, 0, Rgba([0, 0, 0, 10]));
        assert!(matches!(normalize(DynamicImage::ImageRgba8(translucent)), DynamicImage::ImageRgba8(_)));
        let gray = DynamicImage::ImageLuma16(image::ImageBuffer::from_pixel(2, 2, image::Luma([40_000u16])));
        assert!(matches!(normalize(gray), DynamicImage::ImageRgb8(_)));
    }

    #[test]
    fn resize_hits_exact_dimensions() {
        let out = resize(&gradient(37, 21), Dimensions::new(100, 58)).unwrap();
        assert_eq!((out.width(), out.height()), (100, 58));
        let rgba = DynamicImage::ImageRgba8(RgbaImage::from_pixel(10, 10, Rgba([9, 9, 9, 100])));
        let out = resize(&rgba, Dimensions::new(3, 3)).unwrap();
        assert!(matches!(out, DynamicImage::ImageRgba8(_)));
    }

    #[test]
    fn flatten_blends_with_white() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0])));
        assert_eq!(flatten_on_white(&img).get_pixel(0, 0).0, [255, 255, 255]);
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255])));
        assert_eq!(flatten_on_white(&img).get_pixel(0, 0).0, [0, 0, 0]);
    }

    #[test]
    fn every_encoder_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let img = gradient(64, 48);
        for (format, ext) in [(EncodedFormat::Png, "png"), (EncodedFormat::Jpeg, "jpg"), (EncodedFormat::Webp, "webp")]
        {
            let path = dir.path().join(format!("out.{ext}"));
            let saved = save_atomic(&img, format, None, dir.path(), || path.clone()).unwrap();
            let info = read_info(&saved).unwrap();
            assert_eq!(info.dimensions, Dimensions::new(64, 48), "{format:?}");
            let decoded = decode(&saved).unwrap();
            assert_eq!(decoded.dimensions(), Dimensions::new(64, 48));
        }
    }

    #[test]
    fn save_atomic_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let taken = dir.path().join("a.png");
        std::fs::write(&taken, b"original").unwrap();
        let mut candidates = vec![dir.path().join("b.png"), taken.clone()];
        let saved =
            save_atomic(&gradient(8, 8), EncodedFormat::Png, None, dir.path(), || candidates.pop().unwrap()).unwrap();
        assert_eq!(saved, dir.path().join("b.png"));
        assert_eq!(std::fs::read(&taken).unwrap(), b"original");
        // No temp files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn webp_rejects_oversized_images() {
        let wide = DynamicImage::ImageRgb8(RgbImage::new(16_384, 1));
        let err = encode(&wide, EncodedFormat::Webp, None, std::io::sink()).unwrap_err();
        assert!(matches!(err, ImagingError::TooLarge { .. }));
    }

    #[test]
    fn exif_orientation_swaps_reported_dimensions() {
        // Minimal JPEG with an EXIF Orientation=6 (rotate 90° CW) APP1 segment.
        let dir = tempfile::tempdir().unwrap();
        let mut jpeg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg, 90)
            .encode(&[0u8; 4 * 2 * 3], 4, 2, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let exif: [u8; 36] = [
            0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, // APP1 header (length 34)
            b'I', b'I', 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, // TIFF header, IFD at 8
            0x01, 0x00, // 1 entry
            0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, // Orientation = 6
            0x00, 0x00, 0x00, 0x00, // no next IFD
        ];
        let mut with_exif = jpeg[..2].to_vec(); // SOI
        with_exif.extend_from_slice(&exif);
        with_exif.extend_from_slice(&jpeg[2..]);
        let path = dir.path().join("rotated.jpg");
        std::fs::write(&path, &with_exif).unwrap();

        let info = read_info(&path).unwrap();
        assert_eq!(info.dimensions, Dimensions::new(2, 4));
        let decoded = decode(&path).unwrap();
        assert_eq!(decoded.dimensions(), Dimensions::new(2, 4));
    }
}
