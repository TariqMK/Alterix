//! File information and metadata for renaming: file system facts plus lazily read EXIF,
//! audio tags, PDF info, video/audio stream details, image size and content hashes.
//! Property names use common conventions (exif:Make, PDF:Title, MediaInfo:FrameRate)
//! plus Windows-style System.* aliases.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, Metadata};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

type Props = BTreeMap<String, String>;

/// Everything Alterix knows about one file or folder. Metadata groups load on first use.
#[derive(Debug)]
pub struct FileInfo {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub created: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub accessed: Option<NaiveDateTime>,
    pub attrs: String,
    exif: OnceLock<Props>,
    audio: OnceLock<Props>,
    pdf: OnceLock<Props>,
    media: OnceLock<Props>,
    image: OnceLock<Option<(usize, usize)>>,
    hashes: Mutex<HashMap<String, String>>,
}

pub fn to_local(t: SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(t).naive_local()
}

impl FileInfo {
    /// Reads the file system facts for `path`. Metadata groups are read later on demand.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let md = std::fs::symlink_metadata(path)?;
        let md = if md.file_type().is_symlink() {
            std::fs::metadata(path).unwrap_or(md)
        } else {
            md
        };
        Ok(Self::from_metadata(path, &md))
    }

    pub fn from_metadata(path: &Path, md: &Metadata) -> Self {
        FileInfo {
            path: path.to_path_buf(),
            is_dir: md.is_dir(),
            size: if md.is_dir() { 0 } else { md.len() },
            created: md.created().ok().map(to_local),
            modified: md.modified().ok().map(to_local),
            accessed: md.accessed().ok().map(to_local),
            attrs: attr_letters(path, md),
            exif: OnceLock::new(),
            audio: OnceLock::new(),
            pdf: OnceLock::new(),
            media: OnceLock::new(),
            image: OnceLock::new(),
            hashes: Mutex::new(HashMap::new()),
        }
    }

    /// A stand-in used by tests and for paths that no longer exist.
    pub fn virtual_file(path: &Path, is_dir: bool) -> Self {
        FileInfo {
            path: path.to_path_buf(),
            is_dir,
            size: 0,
            created: None,
            modified: None,
            accessed: None,
            attrs: String::new(),
            exif: OnceLock::new(),
            audio: OnceLock::new(),
            pdf: OnceLock::new(),
            media: OnceLock::new(),
            image: OnceLock::new(),
            hashes: Mutex::new(HashMap::new()),
        }
    }

    fn ext_lower(&self) -> String {
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    pub fn exif(&self) -> &Props {
        self.exif.get_or_init(|| {
            if self.is_dir {
                Props::new()
            } else {
                read_exif(&self.path).unwrap_or_default()
            }
        })
    }

    pub fn audio(&self) -> &Props {
        self.audio.get_or_init(|| {
            const AUDIO: &[&str] = &[
                "mp3", "flac", "ogg", "opus", "m4a", "m4b", "aac", "wav", "aif", "aiff", "ape",
                "wv", "mpc", "spx", "wma", "mp4", "m4v", "mov",
            ];
            if self.is_dir || !AUDIO.contains(&self.ext_lower().as_str()) {
                Props::new()
            } else {
                read_audio(&self.path).unwrap_or_default()
            }
        })
    }

    pub fn pdf(&self) -> &Props {
        self.pdf.get_or_init(|| {
            if self.is_dir || self.ext_lower() != "pdf" {
                Props::new()
            } else {
                read_pdf(&self.path).unwrap_or_default()
            }
        })
    }

    pub fn media(&self) -> &Props {
        self.media.get_or_init(|| {
            if self.is_dir {
                return Props::new();
            }
            match self.ext_lower().as_str() {
                "mp4" | "m4v" | "mov" | "m4a" | "3gp" => read_mp4(&self.path).unwrap_or_default(),
                "mkv" | "webm" | "mka" => read_mkv(&self.path).unwrap_or_default(),
                _ => Props::new(),
            }
        })
    }

    pub fn image_size(&self) -> Option<(usize, usize)> {
        *self.image.get_or_init(|| {
            if self.is_dir {
                return None;
            }
            imagesize::size(&self.path)
                .ok()
                .map(|s| (s.width, s.height))
        })
    }

    /// Returns a content hash: crc32, md5, sha1, sha256, sha3 or keccak.
    pub fn hash(&self, algo: &str) -> Option<String> {
        if self.is_dir {
            return None;
        }
        let algo = algo.to_ascii_lowercase();
        if let Some(h) = self.hashes.lock().ok()?.get(&algo) {
            return Some(h.clone());
        }
        let h = compute_hash(&self.path, &algo)?;
        self.hashes.lock().ok()?.insert(algo, h.clone());
        Some(h)
    }

    /// EXIF date taken. `kind` is original, digitized, modified or recent.
    pub fn date_taken(&self, kind: &str) -> Option<NaiveDateTime> {
        let ex = self.exif();
        let get = |k: &str| ex.get(k).and_then(|v| parse_date(v));
        match kind {
            "original" => get("exif:DateTimeOriginal"),
            "digitized" => get("exif:DateTimeDigitized"),
            "modified" => get("exif:DateTime"),
            _ => [
                get("exif:DateTimeOriginal"),
                get("exif:DateTimeDigitized"),
                get("exif:DateTime"),
            ]
            .into_iter()
            .flatten()
            .max(),
        }
    }

    /// The main date of interest: date taken, then media or PDF creation, then created.
    pub fn item_date(&self) -> Option<NaiveDateTime> {
        self.date_taken("original")
            .or_else(|| {
                self.media()
                    .get("media:Created")
                    .and_then(|v| parse_date(v))
            })
            .or_else(|| {
                self.pdf()
                    .get("PDF:CreationDate")
                    .and_then(|v| parse_date(v))
            })
            .or(self.created)
            .or(self.modified)
    }

    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn folder_name(&self) -> String {
        self.path
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Looks up a property by name or label, ignoring case. Covers exif:, PDF:,
    /// MediaInfo:/media:, audio:, hash: and the common Windows System.* names.
    pub fn property(&self, name: &str) -> Option<String> {
        let key = name.trim();
        let lower = key.to_ascii_lowercase();
        if let Some(algo) = lower.strip_prefix("hash:") {
            return self.hash(algo);
        }
        if let Some(v) = self.alias(&lower) {
            return v;
        }
        let find = |p: &Props, k: &str| {
            p.iter()
                .find(|(pk, _)| pk.eq_ignore_ascii_case(k))
                .map(|(_, v)| v.clone())
        };
        if lower.starts_with("exif:") {
            let k = match lower.as_str() {
                "exif:isospeedratings" => "exif:PhotographicSensitivity",
                _ => key,
            };
            return find(self.exif(), k);
        }
        if lower.starts_with("pdf:") {
            return find(self.pdf(), key);
        }
        if let Some(rest) = lower.strip_prefix("mediainfo:") {
            return find(self.media(), &format!("media:{rest}"))
                .or_else(|| find(self.audio(), &format!("audio:{rest}")));
        }
        if lower.starts_with("media:") {
            return find(self.media(), key);
        }
        if lower.starts_with("audio:") {
            return find(self.audio(), key);
        }
        find(self.exif(), &format!("exif:{key}"))
            .or_else(|| find(self.audio(), &format!("audio:{key}")))
            .or_else(|| find(self.pdf(), &format!("PDF:{key}")))
            .or_else(|| find(self.media(), &format!("media:{key}")))
    }

    /// Windows-style property names and labels mapped onto Alterix's readers.
    /// Returns Some(value) when the name is a known alias.
    fn alias(&self, lower: &str) -> Option<Option<String>> {
        let d = |v: Option<NaiveDateTime>| v.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string());
        let first = |vals: Vec<Option<String>>| vals.into_iter().flatten().find(|s| !s.is_empty());
        let ex = |k: &str| self.exif().get(k).cloned();
        let au = |k: &str| self.audio().get(k).cloned();
        let pdf = |k: &str| self.pdf().get(k).cloned();
        let md = |k: &str| self.media().get(k).cloned();
        let width = || {
            self.image_size()
                .map(|s| s.0.to_string())
                .or_else(|| md("media:Width"))
        };
        let height = || {
            self.image_size()
                .map(|s| s.1.to_string())
                .or_else(|| md("media:Height"))
        };
        let v = match lower {
            "system.itemdate" | "item date" => d(self.item_date()),
            "system.photo.datetaken" | "date taken" => d(self.date_taken("original")),
            "system.datecreated" | "date created" => d(self.created),
            "system.datemodified" | "date modified" => d(self.modified),
            "system.dateaccessed" | "date accessed" => d(self.accessed),
            "system.size" | "size" => Some(self.size.to_string()),
            "system.filename" | "filename" => Some(self.name()),
            "system.itemnamedisplay" | "name" => Some(self.name()),
            "system.itemfoldernamedisplay" | "folder name" => Some(self.folder_name()),
            "system.itemfolderpathdisplay" | "folder path" => {
                self.path.parent().map(|p| p.to_string_lossy().into_owned())
            }
            "system.photo.cameramanufacturer" | "camera maker" => ex("exif:Make"),
            "system.photo.cameramodel" | "camera model" => ex("exif:Model"),
            "system.photo.isospeed" | "iso speed" => ex("exif:PhotographicSensitivity"),
            "system.photo.fnumber" | "f-stop" => ex("exif:FNumber"),
            "system.photo.exposuretime" | "exposure time" => ex("exif:ExposureTime"),
            "system.photo.focallength" | "focal length" => ex("exif:FocalLength"),
            "system.photo.orientation" | "orientation" => ex("exif:Orientation"),
            "system.gps.latitude" | "latitude" => ex("exif:GeoLocation.Latitude"),
            "system.gps.longitude" | "longitude" => ex("exif:GeoLocation.Longitude"),
            "system.image.horizontalsize" | "system.video.framewidth" | "width" | "frame width" => {
                width()
            }
            "system.image.verticalsize"
            | "system.video.frameheight"
            | "height"
            | "frame height" => height(),
            "system.image.dimensions" | "dimensions" => match (width(), height()) {
                (Some(w), Some(h)) => Some(format!("{w} x {h}")),
                _ => None,
            },
            "system.title" | "title" => {
                first(vec![au("audio:Title"), pdf("PDF:Title"), md("media:Title")])
            }
            "system.author" | "authors" | "author" => {
                first(vec![pdf("PDF:Author"), au("audio:Artist")])
            }
            "system.subject" | "subject" => pdf("PDF:Subject"),
            "system.keywords" | "tags" | "keywords" => pdf("PDF:Keywords"),
            "system.comment" | "comments" => {
                first(vec![au("audio:Comment"), ex("exif:UserComment")])
            }
            "system.music.artist" | "contributing artists" | "artist" => au("audio:Artist"),
            "system.music.albumartist" | "album artist" => au("audio:AlbumArtist"),
            "system.music.albumtitle" | "album" => au("audio:Album"),
            "system.music.tracknumber" | "#" | "track" => au("audio:Track"),
            "system.music.genre" | "genre" => au("audio:Genre"),
            "system.media.year" | "year" => au("audio:Year"),
            "system.media.duration" | "length" | "duration" => first(vec![
                md("media:Duration/String"),
                au("audio:Duration/String"),
            ]),
            "system.video.framerate" | "frame rate" => md("media:FrameRate"),
            "system.audio.encodingbitrate" | "bit rate" | "total bitrate" => {
                first(vec![au("audio:Bitrate"), md("media:OverallBitRate")])
            }
            "system.audio.samplerate" | "audio sample rate" => au("audio:SampleRate"),
            "system.audio.channelcount" | "channels" => au("audio:Channels"),
            "system.document.pagecount" | "pages" => pdf("PDF:PageCount"),
            "system.applicationname" | "program name" => {
                first(vec![pdf("PDF:Creator"), ex("exif:Software")])
            }
            _ => return None,
        };
        Some(v)
    }

    /// Every property Alterix can read for this file, for the properties view and scripts.
    pub fn all_properties(&self) -> Props {
        let mut out = Props::new();
        let d = |v: Option<NaiveDateTime>| v.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string());
        out.insert("System.FileName".into(), self.name());
        out.insert("System.Size".into(), self.size.to_string());
        if !self.attrs.is_empty() {
            out.insert("Attributes".into(), self.attrs.clone());
        }
        for (k, v) in [
            ("System.DateCreated", d(self.created)),
            ("System.DateModified", d(self.modified)),
            ("System.DateAccessed", d(self.accessed)),
            ("System.ItemDate", d(self.item_date())),
        ] {
            if let Some(v) = v {
                out.insert(k.into(), v);
            }
        }
        if let Some((w, h)) = self.image_size() {
            out.insert("System.Image.Dimensions".into(), format!("{w} x {h}"));
        }
        for group in [self.exif(), self.audio(), self.pdf(), self.media()] {
            out.extend(group.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        out
    }
}

/// Attribute letters: D R H S A.
fn attr_letters(path: &Path, md: &Metadata) -> String {
    let mut s = String::new();
    if md.is_dir() {
        s.push('D');
    }
    if md.permissions().readonly() {
        s.push('R');
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let a = md.file_attributes();
        if a & 0x2 != 0 {
            s.push('H');
        }
        if a & 0x4 != 0 {
            s.push('S');
        }
        if a & 0x20 != 0 {
            s.push('A');
        }
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        const UF_HIDDEN: u32 = 0x8000;
        if md.st_flags() & UF_HIDDEN != 0 {
            s.push('H');
        }
    }
    #[cfg(not(windows))]
    {
        if path
            .file_name()
            .map(|n| n.to_string_lossy().starts_with('.'))
            .unwrap_or(false)
            && !s.contains('H')
        {
            s.push('H');
        }
    }
    let _ = path;
    s
}

/// Parses dates from EXIF ("2024:06:14 09:05:07"), PDF ("D:20240614090507+01'00'"),
/// ISO 8601 and plain "YYYY-MM-DD" forms.
pub fn parse_date(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim().trim_matches('"');
    let s = s.strip_prefix("D:").unwrap_or(s);
    const FORMATS: &[&str] = &[
        "%Y:%m:%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y%m%d%H%M%S",
        "%Y-%m-%d %H:%M",
    ];
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Local).naive_local());
    }
    for f in FORMATS {
        let take = s.len().min(f.replace('%', "").len() + 6);
        for cut in [s.len(), take] {
            if let Some(part) = s.get(..cut) {
                if let Ok(dt) = NaiveDateTime::parse_from_str(part, f) {
                    return Some(dt);
                }
            }
        }
    }
    if s.len() >= 14 && s[..14].chars().all(|c| c.is_ascii_digit()) {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&s[..14], "%Y%m%d%H%M%S") {
            return Some(dt);
        }
    }
    for f in ["%Y-%m-%d", "%Y:%m:%d", "%Y%m%d"] {
        if let Some(part) = s.get(..s.len().min(10)) {
            if let Ok(d) = NaiveDate::parse_from_str(part, f) {
                return d.and_hms_opt(0, 0, 0);
            }
        }
    }
    None
}

/// Keeps the digits (and one decimal point / leading minus) of a value, for the # marker.
pub fn as_number(s: &str) -> Option<f64> {
    let mut out = String::new();
    let mut seen_dot = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            out.push(c);
        } else if c == '.' && !seen_dot && !out.is_empty() {
            out.push(c);
            seen_dot = true;
        } else if c == '-' && out.is_empty() {
            out.push(c);
        } else if c == '/' && !out.is_empty() {
            // Rational values such as 1/250.
            let (a, b) = s.split_once('/')?;
            let a: f64 = a.trim().parse().ok()?;
            let b: f64 = b.trim().parse().ok()?;
            return if b != 0.0 { Some(a / b) } else { None };
        }
    }
    out.trim_end_matches('.').parse().ok()
}

// ---------------------------------------------------------------- readers

fn read_exif(path: &Path) -> Option<Props> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let ex = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let mut out = Props::new();
    for f in ex.fields() {
        if f.ifd_num != exif::In::PRIMARY {
            continue;
        }
        let name = format!("exif:{}", f.tag);
        let value = f
            .display_value()
            .to_string()
            .trim_matches('"')
            .trim()
            .to_string();
        if !value.is_empty() && !value.starts_with("0x") {
            out.entry(name).or_insert(value);
        }
    }
    // Derived values: image size, resolution, ISO and GPS in decimal degrees.
    let num = |k: &str| out.get(k).and_then(|v| as_number(v));
    if let (Some(w), Some(h)) = (
        num("exif:PixelXDimension").or(num("exif:ImageWidth")),
        num("exif:PixelYDimension").or(num("exif:ImageLength")),
    ) {
        out.insert("exif:ImageWidth".into(), format!("{w}"));
        out.insert("exif:ImageHeight".into(), format!("{h}"));
        out.insert("exif:ImageResolution".into(), format!("{w}x{h}"));
    }
    if let Some(v) = out.get("exif:PhotographicSensitivity").cloned() {
        out.insert("exif:ISOSpeedRatings".into(), v);
    }
    for (axis, reference) in [
        ("Latitude", "exif:GPSLatitudeRef"),
        ("Longitude", "exif:GPSLongitudeRef"),
    ] {
        let field = ex.get_field(
            if axis == "Latitude" {
                exif::Tag::GPSLatitude
            } else {
                exif::Tag::GPSLongitude
            },
            exif::In::PRIMARY,
        );
        if let Some(exif::Field {
            value: exif::Value::Rational(r),
            ..
        }) = field
        {
            if r.len() == 3 {
                let mut deg = r[0].to_f64() + r[1].to_f64() / 60.0 + r[2].to_f64() / 3600.0;
                if matches!(
                    out.get(reference).map(String::as_str),
                    Some("S") | Some("W")
                ) {
                    deg = -deg;
                }
                out.insert(format!("exif:GeoLocation.{axis}"), format!("{deg:.6}"));
            }
        }
    }
    Some(out)
}

fn read_audio(path: &Path) -> Option<Props> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(path).ok()?;
    let mut out = Props::new();
    let props = tagged.properties();
    let dur = props.duration();
    if !dur.is_zero() {
        let secs = dur.as_secs();
        out.insert("audio:Duration".into(), format!("{}", dur.as_millis()));
        out.insert(
            "audio:Duration/String".into(),
            if secs >= 3600 {
                format!("{}h {:02}m {:02}s", secs / 3600, secs / 60 % 60, secs % 60)
            } else {
                format!("{}m {:02}s", secs / 60, secs % 60)
            },
        );
    }
    if let Some(b) = props.audio_bitrate() {
        out.insert("audio:Bitrate".into(), format!("{b} kbps"));
    }
    if let Some(r) = props.sample_rate() {
        out.insert("audio:SampleRate".into(), r.to_string());
    }
    if let Some(c) = props.channels() {
        out.insert("audio:Channels".into(), c.to_string());
    }
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        let mut put = |k: &str, v: Option<String>| {
            if let Some(v) = v.filter(|v| !v.trim().is_empty()) {
                out.insert(format!("audio:{k}"), v.trim().to_string());
            }
        };
        put("Artist", tag.artist().map(|s| s.to_string()));
        put("Album", tag.album().map(|s| s.to_string()));
        put("Title", tag.title().map(|s| s.to_string()));
        put("Genre", tag.genre().map(|s| s.to_string()));
        put("Comment", tag.comment().map(|s| s.to_string()));
        put("Track", tag.track().map(|n| n.to_string()));
        put("TrackTotal", tag.track_total().map(|n| n.to_string()));
        put("Disc", tag.disk().map(|n| n.to_string()));
        put("Year", tag.date().map(|d| d.year.to_string()));
        put(
            "AlbumArtist",
            tag.get_string(lofty::tag::ItemKey::AlbumArtist)
                .map(|s| s.to_string()),
        );
        put(
            "Composer",
            tag.get_string(lofty::tag::ItemKey::Composer)
                .map(|s| s.to_string()),
        );
    }
    Some(out)
}

fn read_pdf(path: &Path) -> Option<Props> {
    let doc = lopdf::Document::load(path).ok()?;
    let mut out = Props::new();
    out.insert("PDF:PageCount".into(), doc.get_pages().len().to_string());
    out.insert("PDF:Version".into(), doc.version.clone());
    let info_ref = doc.trailer.get(b"Info").ok()?.as_reference().ok()?;
    let info = doc.get_object(info_ref).ok()?.as_dict().ok()?;
    for (k, v) in info.iter() {
        let key = String::from_utf8_lossy(k).to_string();
        let text = match v {
            lopdf::Object::String(bytes, _) => decode_pdf_string(bytes),
            lopdf::Object::Name(n) => String::from_utf8_lossy(n).to_string(),
            _ => continue,
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        if key.ends_with("Date") {
            if let Some(dt) = parse_date(&text) {
                out.insert(
                    format!("PDF:{key}"),
                    dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                );
                continue;
            }
        }
        out.insert(format!("PDF:{key}"), text);
    }
    Some(out)
}

fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        // PDFDocEncoding matches Latin-1 for printable characters.
        bytes.iter().map(|&b| b as char).collect()
    }
}

fn duration_string(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{} h {} min", s / 3600, s / 60 % 60)
    } else if s >= 60 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{s} s")
    }
}

fn read_mp4(path: &Path) -> Option<Props> {
    let f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mp4 = mp4::Mp4Reader::read_header(BufReader::new(f), size).ok()?;
    let mut out = Props::new();
    let ms = mp4.duration().as_millis() as u64;
    out.insert("media:Duration".into(), ms.to_string());
    out.insert("media:Duration/String".into(), duration_string(ms));
    if let Some(rate) = (size * 8 * 1000).checked_div(ms) {
        out.insert("media:OverallBitRate".into(), rate.to_string());
    }
    let created = mp4.moov.mvhd.creation_time;
    if created > 2_082_844_800 {
        // Seconds since 1904-01-01 UTC.
        let unix = created as i64 - 2_082_844_800;
        if let Some(dt) = DateTime::from_timestamp(unix, 0) {
            out.insert(
                "media:Created".into(),
                dt.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            );
        }
    }
    let mut video = Vec::new();
    let mut audio = Vec::new();
    for track in mp4.tracks().values() {
        match track.track_type() {
            Ok(mp4::TrackType::Video) => {
                out.entry("media:Width".into())
                    .or_insert(track.width().to_string());
                out.entry("media:Height".into())
                    .or_insert(track.height().to_string());
                let fr = track.frame_rate();
                if fr > 0.0 {
                    out.entry("media:FrameRate".into())
                        .or_insert(format!("{fr:.3}"));
                }
                if let Ok(t) = track.media_type() {
                    video.push(t.to_string());
                }
            }
            Ok(mp4::TrackType::Audio) => {
                if let Ok(t) = track.media_type() {
                    audio.push(t.to_string());
                }
            }
            _ => {}
        }
    }
    if !video.is_empty() {
        out.insert("media:Video_Format_List".into(), video.join(" / "));
    }
    if !audio.is_empty() {
        out.insert("media:Audio_Format_List".into(), audio.join(" / "));
    }
    Some(out)
}

fn read_mkv(path: &Path) -> Option<Props> {
    let f = File::open(path).ok()?;
    let mkv = matroska::Matroska::open(f).ok()?;
    let mut out = Props::new();
    if let Some(d) = mkv.info.duration {
        let ms = d.as_millis() as u64;
        out.insert("media:Duration".into(), ms.to_string());
        out.insert("media:Duration/String".into(), duration_string(ms));
    }
    if let Some(t) = &mkv.info.title {
        out.insert("media:Title".into(), t.clone());
    }
    if let Some(dt) = &mkv.info.date_utc {
        // Nanoseconds since 2001-01-01 UTC (978307200 Unix seconds).
        let nanos: i64 = dt.clone().into();
        if let Some(t) = DateTime::from_timestamp(978_307_200 + nanos / 1_000_000_000, 0) {
            out.insert(
                "media:Created".into(),
                t.with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            );
        }
    }
    let mut video = Vec::new();
    let mut audio = Vec::new();
    for t in &mkv.tracks {
        match &t.settings {
            matroska::Settings::Video(v) => {
                out.entry("media:Width".into())
                    .or_insert(v.pixel_width.to_string());
                out.entry("media:Height".into())
                    .or_insert(v.pixel_height.to_string());
                video.push(t.codec_id.trim_start_matches("V_").to_string());
            }
            matroska::Settings::Audio(a) => {
                out.entry("media:SamplingRate".into())
                    .or_insert(format!("{}", a.sample_rate));
                out.entry("media:Channels".into())
                    .or_insert(a.channels.to_string());
                audio.push(t.codec_id.trim_start_matches("A_").to_string());
            }
            _ => {}
        }
    }
    if !video.is_empty() {
        out.insert("media:Video_Format_List".into(), video.join(" / "));
    }
    if !audio.is_empty() {
        out.insert("media:Audio_Format_List".into(), audio.join(" / "));
    }
    Some(out)
}

fn compute_hash(path: &Path, algo: &str) -> Option<String> {
    use sha2::Digest as _;
    let mut f = BufReader::new(File::open(path).ok()?);
    let mut buf = vec![0u8; 1 << 16];
    macro_rules! digest {
        ($h:expr) => {{
            let mut h = $h;
            loop {
                let n = f.read(&mut buf).ok()?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Some(hex(&h.finalize()))
        }};
    }
    match algo {
        "crc32" => {
            let mut h = crc32fast::Hasher::new();
            loop {
                let n = f.read(&mut buf).ok()?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Some(format!("{:08x}", h.finalize()))
        }
        "md5" => digest!(md5::Md5::new()),
        "sha1" => digest!(sha1::Sha1::new()),
        "sha256" => digest!(sha2::Sha256::new()),
        "sha3" => digest!(sha3::Sha3_256::new()),
        "keccak" => digest!(sha3::Keccak256::new()),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_date_forms() {
        let want = NaiveDate::from_ymd_opt(2008, 10, 22)
            .unwrap()
            .and_hms_opt(16, 46, 53)
            .unwrap();
        assert_eq!(parse_date("2008:10:22 16:46:53"), Some(want));
        assert_eq!(parse_date("D:20081022164653+01'00'"), Some(want));
        assert_eq!(parse_date("2008-10-22 16:46:53"), Some(want));
        assert!(parse_date("not a date").is_none());
    }

    #[test]
    fn numbers_keep_digits_only() {
        assert_eq!(as_number("ISO100"), Some(100.0));
        assert_eq!(as_number("v2.0-beta"), Some(2.0));
        assert_eq!(as_number("1/250"), Some(0.004));
        assert_eq!(as_number("f/5.6"), Some(5.6));
    }

    #[test]
    fn hashes_and_aliases_work_on_a_real_file() {
        let dir = std::env::temp_dir().join(format!("alterix-meta-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("hello.txt");
        std::fs::write(&p, "hello").unwrap();
        let info = FileInfo::load(&p).unwrap();
        assert_eq!(
            info.hash("md5").as_deref(),
            Some("5d41402abc4b2a76b9719d911017c592")
        );
        assert_eq!(info.hash("crc32").as_deref(), Some("3610a686"));
        assert_eq!(
            info.property("hash:sha256").as_deref(),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
        assert_eq!(info.property("System.Size").as_deref(), Some("5"));
        assert_eq!(
            info.property("folder name").as_deref(),
            dir.file_name().and_then(|n| n.to_str())
        );
        assert!(info.property("exif:Make").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
