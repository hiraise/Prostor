//! Files (D-033): names, what kind a file is, how it is served, and the signed links that open
//! attachments (CNT-026).
//!
//! The kind comes from the file's first bytes, never from what the client said, and only kinds a
//! browser shows safely open in the page: raster pictures, three video formats and PDF. Everything
//! else, SVG and HTML among them, downloads as `application/octet-stream`.
//!
//! A link names the attachment, the person it was made for and its expiry, signed with the
//! instance secret over the attachment's and the person's own secrets. Serving checks the
//! signature and then that person's current access, so a link never shows more than its person
//! may see now, and a new secret on either side revokes every link made before.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use time::{Date, Duration, OffsetDateTime, UtcOffset};

use crate::validation::{Problem, Rejection};
use crate::{AttachmentId, UserId};

/// Longest file name, in characters.
pub const NAME_MAX: usize = 255;
/// Most files one issue keeps, its comments' included.
pub const ISSUE_FILES_MAX: usize = 1_000;
/// Most uploads one person may keep staged.
pub const STAGED_MAX: i64 = 100;
/// How long a staged upload waits to be attached, in hours.
pub const STAGED_HOURS: i64 = 24;
/// Largest picture decoded for an avatar, logo or thumbnail, in pixels.
pub const IMAGE_PIXELS_MAX: u64 = 40_000_000;

/// Validates a file name (CNT-017): 1–255 characters, no `/`, `\` or control characters; white
/// space around it is removed.
pub fn name(raw: &str) -> Result<String, Rejection> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(Rejection::new(Problem::Required, "the file name is empty"));
    }
    if name.chars().count() > NAME_MAX {
        return Err(Rejection::new(
            Problem::TooLong,
            format!("a file name is longer than {NAME_MAX} characters"),
        ));
    }
    if name
        .chars()
        .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return Err(Rejection::new(
            Problem::Invalid,
            "a file name may not contain /, \\ or control characters",
        ));
    }
    Ok(name.to_owned())
}

/// The name a new file gets next to `existing` ones of the same issue: the wanted name, or with
/// ` (2)`, ` (3)`… before the extension when it is taken (names compare ignoring case).
pub fn unique_name<S: AsRef<str>>(existing: &[S], wanted: &str) -> String {
    let taken = |candidate: &str| {
        existing
            .iter()
            .any(|e| e.as_ref().to_lowercase() == candidate.to_lowercase())
    };
    if !taken(wanted) {
        return wanted.to_owned();
    }
    let (stem, extension) = match wanted.rfind('.') {
        Some(at) if at > 0 => wanted.split_at(at),
        _ => (wanted, ""),
    };
    let mut number = 2;
    loop {
        let candidate = format!("{stem} ({number}){extension}");
        if !taken(&candidate) {
            return candidate;
        }
        number += 1;
    }
}

/// What a file is, by its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Mp4,
    Webm,
    Ogg,
    Pdf,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 9] = [
        Kind::Png,
        Kind::Jpeg,
        Kind::Gif,
        Kind::Webp,
        Kind::Mp4,
        Kind::Webm,
        Kind::Ogg,
        Kind::Pdf,
        Kind::Other,
    ];

    /// The content type it is served with.
    pub fn content_type(self) -> &'static str {
        match self {
            Kind::Png => "image/png",
            Kind::Jpeg => "image/jpeg",
            Kind::Gif => "image/gif",
            Kind::Webp => "image/webp",
            Kind::Mp4 => "video/mp4",
            Kind::Webm => "video/webm",
            Kind::Ogg => "video/ogg",
            Kind::Pdf => "application/pdf",
            Kind::Other => "application/octet-stream",
        }
    }

    /// The kind of a stored content type; unknown types are `Other`.
    pub fn of_content_type(content_type: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|k| k.content_type() == content_type)
            .unwrap_or(Kind::Other)
    }

    /// The extension of files the server writes in this format.
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Png => "png",
            Kind::Jpeg => "jpg",
            Kind::Gif => "gif",
            Kind::Webp => "webp",
            Kind::Mp4 => "mp4",
            Kind::Webm => "webm",
            Kind::Ogg => "ogg",
            Kind::Pdf => "pdf",
            Kind::Other => "",
        }
    }

    /// A raster picture the server can decode (thumbnails, avatars, logos).
    pub fn is_image(self) -> bool {
        matches!(self, Kind::Png | Kind::Jpeg | Kind::Gif | Kind::Webp)
    }

    pub fn is_video(self) -> bool {
        matches!(self, Kind::Mp4 | Kind::Webm | Kind::Ogg)
    }

    /// Opens in the browser rather than downloading.
    pub fn inline(self) -> bool {
        self != Kind::Other
    }
}

/// Recognizes a file by its first bytes (CNT-017). Never trusts the client's type.
pub fn sniff(head: &[u8]) -> Kind {
    let starts = |magic: &[u8]| head.starts_with(magic);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Kind::Png
    } else if starts(b"\xff\xd8\xff") {
        Kind::Jpeg
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Kind::Gif
    } else if starts(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        Kind::Webp
    } else if head.get(4..8) == Some(b"ftyp") && head.get(8..12) != Some(b"qt  ") {
        // ISO media: MP4 and its kin. QuickTime files are not something browsers play.
        Kind::Mp4
    } else if starts(b"\x1a\x45\xdf\xa3") {
        Kind::Webm
    } else if starts(b"OggS") {
        Kind::Ogg
    } else if starts(b"%PDF-") {
        Kind::Pdf
    } else {
        Kind::Other
    }
}

/// Until when links given in API answers work: the end of the third UTC day after `now`. Every
/// answer of one day gives the same links, so browsers can cache the files.
pub fn ui_link_expiry(now: OffsetDateTime) -> OffsetDateTime {
    end_of_day(now, 3)
}

/// Until when links in emails work (1.10): two weeks, to the end of that UTC day.
pub fn email_link_expiry(now: OffsetDateTime) -> OffsetDateTime {
    end_of_day(now, 14)
}

fn end_of_day(now: OffsetDateTime, days: i64) -> OffsetDateTime {
    let date: Date = now.to_offset(UtcOffset::UTC).date();
    let next = date.saturating_add(Duration::days(days + 1));
    next.midnight().assume_utc()
}

/// What a link opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link<'a> {
    pub attachment: AttachmentId,
    pub attachment_secret: &'a [u8],
    /// Whom the link was made for: serving checks their access.
    pub user: UserId,
    pub user_secret: &'a [u8],
    /// Unix seconds.
    pub expires: i64,
    /// The thumbnail rather than the file.
    pub thumbnail: bool,
}

fn mac(instance_secret: &[u8], link: &Link<'_>) -> Hmac<Sha256> {
    // HMAC accepts keys of any length.
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(instance_secret)
        .unwrap_or_else(|_| unreachable_key());
    mac.update(b"taskmaster file link v1\0");
    mac.update(&link.attachment.to_be_bytes());
    mac.update(&(link.attachment_secret.len() as u64).to_be_bytes());
    mac.update(link.attachment_secret);
    mac.update(&link.user.to_be_bytes());
    mac.update(&(link.user_secret.len() as u64).to_be_bytes());
    mac.update(link.user_secret);
    mac.update(&link.expires.to_be_bytes());
    mac.update(&[u8::from(link.thumbnail)]);
    mac
}

#[cold]
fn unreachable_key() -> Hmac<Sha256> {
    // `new_from_slice` fails only for fixed-size keys; HMAC has none. Keep a fixed key rather
    // than panic: such links would simply never verify against the instance secret.
    <Hmac<Sha256> as KeyInit>::new(&Default::default())
}

/// The signature of a link.
pub fn sign(instance_secret: &[u8], link: &Link<'_>) -> [u8; 32] {
    mac(instance_secret, link).finalize().into_bytes().into()
}

/// Whether a signature is the link's, compared in constant time; expiry is checked separately.
pub fn verify(instance_secret: &[u8], link: &Link<'_>, signature: &[u8]) -> bool {
    mac(instance_secret, link).verify_slice(signature).is_ok()
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn cnt_017_names() {
        assert_eq!(name("  report.pdf ").unwrap(), "report.pdf");
        assert_eq!(name(" ").unwrap_err().problem, Problem::Required);
        assert_eq!(name("a/b.txt").unwrap_err().problem, Problem::Invalid);
        assert_eq!(name("a\\b.txt").unwrap_err().problem, Problem::Invalid);
        assert_eq!(name("a\u{7}b").unwrap_err().problem, Problem::Invalid);
        assert!(name(&"я".repeat(NAME_MAX)).is_ok());
        assert_eq!(
            name(&"я".repeat(NAME_MAX + 1)).unwrap_err().problem,
            Problem::TooLong
        );
    }

    #[test]
    fn cnt_017_names_already_taken_get_a_number() {
        let existing = ["shot.png", "Shot (2).PNG", "notes", ".env"];
        assert_eq!(unique_name(&existing, "log.txt"), "log.txt");
        assert_eq!(unique_name(&existing, "SHOT.png"), "SHOT (3).png");
        assert_eq!(unique_name(&existing, "notes"), "notes (2)");
        assert_eq!(unique_name(&existing, ".env"), ".env (2)");
        assert_eq!(unique_name(&existing, "a.tar.gz"), "a.tar.gz");
        assert_eq!(
            unique_name(&["a.tar.gz".to_owned()], "a.tar.gz"),
            "a.tar (2).gz"
        );
    }

    #[test]
    fn cnt_017_sniffing_trusts_bytes_only() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Kind::Png);
        assert_eq!(sniff(b"\xff\xd8\xff\xe0"), Kind::Jpeg);
        assert_eq!(sniff(b"GIF89a.."), Kind::Gif);
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Kind::Webp);
        assert_eq!(sniff(b"RIFF\0\0\0\0WAVEfmt "), Kind::Other);
        assert_eq!(sniff(b"\0\0\0\x18ftypmp42"), Kind::Mp4);
        assert_eq!(sniff(b"\0\0\0\x14ftypqt  "), Kind::Other);
        assert_eq!(sniff(b"\x1a\x45\xdf\xa3...."), Kind::Webm);
        assert_eq!(sniff(b"OggS\0"), Kind::Ogg);
        assert_eq!(sniff(b"%PDF-1.7"), Kind::Pdf);
        // Markup that browsers would run downloads instead.
        assert_eq!(
            sniff(b"<svg xmlns=\"http://www.w3.org/2000/svg\">"),
            Kind::Other
        );
        assert_eq!(sniff(b"<!doctype html>"), Kind::Other);
        assert_eq!(sniff(b""), Kind::Other);
        assert_eq!(Kind::Other.content_type(), "application/octet-stream");
        assert!(!Kind::Other.inline());
        for kind in Kind::ALL {
            assert_eq!(Kind::of_content_type(kind.content_type()), kind);
        }
        assert_eq!(Kind::of_content_type("image/svg+xml"), Kind::Other);
    }

    #[test]
    fn cnt_026_expiry_is_stable_for_a_day() {
        let morning = datetime!(2026-10-22 00:00:01 UTC);
        let evening = datetime!(2026-10-22 23:59:59 UTC);
        assert_eq!(ui_link_expiry(morning), datetime!(2026-10-26 00:00 UTC));
        assert_eq!(ui_link_expiry(evening), ui_link_expiry(morning));
        // Another offset, the same UTC day.
        assert_eq!(
            ui_link_expiry(datetime!(2026-10-23 01:00 +03:00)),
            datetime!(2026-10-26 00:00 UTC)
        );
        assert_eq!(email_link_expiry(morning), datetime!(2026-11-06 00:00 UTC));
    }

    #[test]
    fn cnt_026_signatures() {
        let link = Link {
            attachment: 7,
            attachment_secret: b"file secret",
            user: 5,
            user_secret: b"user secret",
            expires: 1_800_000_000,
            thumbnail: false,
        };
        let signature = sign(b"instance", &link);
        assert!(verify(b"instance", &link, &signature));
        assert!(!verify(b"another instance", &link, &signature));
        assert!(!verify(b"instance", &link, &signature[..31]));
        for changed in [
            Link {
                attachment: 8,
                ..link
            },
            Link { user: 6, ..link },
            Link {
                expires: link.expires + 1,
                ..link
            },
            Link {
                thumbnail: true,
                ..link
            },
            Link {
                attachment_secret: b"new file secret",
                ..link
            },
            Link {
                user_secret: b"new user secret",
                ..link
            },
        ] {
            assert!(!verify(b"instance", &changed, &signature), "{changed:?}");
        }
    }
}
