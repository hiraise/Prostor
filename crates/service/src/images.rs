//! Pictures (CNT-022, ACC-017, PRJ-007): sizes and thumbnails of attached pictures, avatars and
//! project logos. Decoding is CPU work and runs on a blocking thread, with limits on the picture's
//! size and on memory, and the EXIF orientation applied so photos stand upright.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbaImage};
use prostor_domain::files::{IMAGE_PIXELS_MAX, Kind};
use prostor_domain::validation::Problem;

use crate::{Error, Result};

/// Thumbnails fit inside this square (CNT-022).
pub const THUMBNAIL_SIDE: u32 = 480;
/// Avatars and logos are this square.
pub const PICTURE_SIDE: u32 = 256;
/// Most memory one decoding may take.
const ALLOCATION_MAX: u64 = 512 * 1024 * 1024;

/// What inspecting a picture found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspected {
    pub width: u32,
    pub height: u32,
    /// A smaller copy when the picture is larger than the thumbnail square.
    pub thumbnail: Option<Encoded>,
}

/// A picture the server made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub kind: Kind,
}

/// Why a picture cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unusable {
    /// Not a picture in a format we read, or a broken one.
    NotAPicture,
    /// More pixels than [`IMAGE_PIXELS_MAX`].
    TooLarge,
}

impl Unusable {
    /// The refusal for an upload given as `path`.
    pub fn error(&self, path: &str) -> Error {
        match self {
            Unusable::NotAPicture => Error::field(
                path,
                Problem::Invalid,
                "the file is not a PNG, JPEG, GIF or WebP picture",
            ),
            Unusable::TooLarge => Error::field(
                path,
                Problem::OutOfRange,
                format!("the picture has more than {IMAGE_PIXELS_MAX} pixels"),
            ),
        }
    }
}

fn format_of(kind: Kind) -> Option<ImageFormat> {
    match kind {
        Kind::Png => Some(ImageFormat::Png),
        Kind::Jpeg => Some(ImageFormat::Jpeg),
        Kind::Gif => Some(ImageFormat::Gif),
        Kind::Webp => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// Decodes a picture of a known kind with limits, upright. A GIF gives its first frame.
pub fn decode(bytes: &[u8], kind: Kind) -> Result<DynamicImage, Unusable> {
    let format = format_of(kind).ok_or(Unusable::NotAPicture)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_alloc = Some(ALLOCATION_MAX);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| Unusable::NotAPicture)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > IMAGE_PIXELS_MAX {
        return Err(Unusable::TooLarge);
    }
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| Unusable::NotAPicture)?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    Ok(image)
}

/// JPEG for opaque pictures (smaller), PNG for those with transparency.
fn encode(image: &DynamicImage, quality: u8) -> Result<Encoded, Unusable> {
    let mut out = Cursor::new(Vec::new());
    if image.color().has_alpha() {
        image
            .write_to(&mut out, ImageFormat::Png)
            .map_err(|_| Unusable::NotAPicture)?;
        return Ok(Encoded {
            bytes: out.into_inner(),
            kind: Kind::Png,
        });
    }
    let rgb = image.to_rgb8();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    rgb.write_with_encoder(encoder)
        .map_err(|_| Unusable::NotAPicture)?;
    Ok(Encoded {
        bytes: out.into_inner(),
        kind: Kind::Jpeg,
    })
}

fn png(image: &DynamicImage) -> Result<Encoded, Unusable> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|_| Unusable::NotAPicture)?;
    Ok(Encoded {
        bytes: out.into_inner(),
        kind: Kind::Png,
    })
}

/// The size of an attached picture and its thumbnail (CNT-022).
pub fn inspect(bytes: &[u8], kind: Kind) -> Result<Inspected, Unusable> {
    let image = decode(bytes, kind)?;
    let (width, height) = (image.width(), image.height());
    let thumbnail = if width > THUMBNAIL_SIDE || height > THUMBNAIL_SIDE {
        Some(encode(
            &image.thumbnail(THUMBNAIL_SIDE, THUMBNAIL_SIDE),
            80,
        )?)
    } else {
        None
    };
    Ok(Inspected {
        width,
        height,
        thumbnail,
    })
}

/// An avatar (ACC-017): the middle square of the picture, 256 × 256.
pub fn avatar(bytes: &[u8], kind: Kind) -> Result<Encoded, Unusable> {
    let image = decode(bytes, kind)?;
    let side = image.width().min(image.height());
    let x = (image.width() - side) / 2;
    let y = (image.height() - side) / 2;
    let square = image.crop_imm(x, y, side, side).resize_exact(
        PICTURE_SIDE,
        PICTURE_SIDE,
        FilterType::Lanczos3,
    );
    encode(&square, 85)
}

/// A project logo (PRJ-007): the whole picture inside 256 × 256, centered on transparency, PNG.
pub fn logo(bytes: &[u8], kind: Kind) -> Result<Encoded, Unusable> {
    let image = decode(bytes, kind)?;
    let fitted = image.resize(PICTURE_SIDE, PICTURE_SIDE, FilterType::Lanczos3);
    let mut canvas = RgbaImage::new(PICTURE_SIDE, PICTURE_SIDE);
    let x = i64::from((PICTURE_SIDE - fitted.width()) / 2);
    let y = i64::from((PICTURE_SIDE - fitted.height()) / 2);
    image::imageops::overlay(&mut canvas, &fitted.to_rgba8(), x, y);
    png(&DynamicImage::ImageRgba8(canvas))
}

/// Runs picture work on a blocking thread.
pub async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Unusable> + Send + 'static,
) -> Result<Result<T, Unusable>> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::Internal(format!("picture work stopped: {e}")))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use image::{Rgb, RgbImage, Rgba};

    use super::*;

    fn picture(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([200, 0, 0])));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    #[test]
    fn cnt_022_sizes_and_thumbnails() {
        let big = picture(1200, 600, ImageFormat::Png);
        let found = inspect(&big, Kind::Png).unwrap();
        assert_eq!((found.width, found.height), (1200, 600));
        let thumbnail = found.thumbnail.unwrap();
        assert_eq!(thumbnail.kind, Kind::Jpeg);
        let small = image::load_from_memory(&thumbnail.bytes).unwrap();
        assert_eq!((small.width(), small.height()), (480, 240));
        // Small pictures need none.
        let little = inspect(&picture(300, 200, ImageFormat::Jpeg), Kind::Jpeg).unwrap();
        assert_eq!(
            (little.width, little.height, little.thumbnail),
            (300, 200, None)
        );
        // Transparency stays: PNG.
        let mut clear = RgbaImage::from_pixel(600, 600, Rgba([0, 0, 0, 0]));
        clear.put_pixel(1, 1, Rgba([1, 2, 3, 255]));
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(clear)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        let found = inspect(&out.into_inner(), Kind::Png).unwrap();
        assert_eq!(found.thumbnail.unwrap().kind, Kind::Png);
    }

    #[test]
    fn cnt_022_broken_and_foreign_files() {
        assert_eq!(
            inspect(b"\x89PNG\r\n\x1a\nbroken", Kind::Png),
            Err(Unusable::NotAPicture)
        );
        assert_eq!(inspect(b"%PDF-1.7", Kind::Pdf), Err(Unusable::NotAPicture));
    }

    #[test]
    fn acc_017_avatars_are_middle_squares() {
        let avatar = avatar(&picture(900, 300, ImageFormat::Png), Kind::Png).unwrap();
        let decoded = image::load_from_memory(&avatar.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (256, 256));
    }

    #[test]
    fn prj_007_logos_keep_the_whole_picture() {
        let logo = logo(&picture(512, 128, ImageFormat::Jpeg), Kind::Jpeg).unwrap();
        assert_eq!(logo.kind, Kind::Png);
        let decoded = image::load_from_memory(&logo.bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (256, 256));
        // Padding above, the picture in the middle.
        assert_eq!(decoded.get_pixel(128, 0)[3], 0);
        assert_eq!(decoded.get_pixel(128, 128)[3], 255);
    }
}
