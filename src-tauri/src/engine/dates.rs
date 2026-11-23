//! Date formatting and parsing: % codes, { } token formats, formatter date patterns,
//! JavaScript formatDateTime patterns, Auto Date presets and "Reformat Date" in names.

use chrono::format::{Item, StrftimeItems};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone};
use std::fmt::Write;

/// Formats with strftime-style % codes. Unknown codes are kept as typed instead of failing.
pub fn strftime(dt: &NaiveDateTime, fmt: &str) -> String {
    let items: Vec<Item> = StrftimeItems::new(fmt).collect();
    if items.iter().any(|i| matches!(i, Item::Error)) {
        // Fall back one code at a time so a single bad code does not lose the rest.
        let mut out = String::new();
        let mut chars = fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                let mut code = String::from("%");
                if let Some(&n) = chars.peek() {
                    code.push(n);
                    chars.next();
                    if matches!(n, '-' | '_' | '0') {
                        if let Some(&m) = chars.peek() {
                            code.push(m);
                            chars.next();
                        }
                    }
                }
                let one: Vec<Item> = StrftimeItems::new(&code).collect();
                if one.iter().any(|i| matches!(i, Item::Error)) {
                    out.push_str(&code);
                } else {
                    let _ = write!(out, "{}", format_local(dt, &code));
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    format_local(dt, fmt)
}

fn format_local(dt: &NaiveDateTime, fmt: &str) -> String {
    // %z needs an offset; use the local zone for the given time.
    match Local.from_local_datetime(dt).earliest() {
        Some(local) => {
            let mut s = String::new();
            let _ = write!(s, "{}", local.format(fmt));
            s
        }
        None => {
            let mut s = String::new();
            let _ = write!(s, "{}", dt.format(&fmt.replace("%z", "")));
            s
        }
    }
}

type TokenTable = &'static [(&'static str, &'static str)];

/// Tokens of the `{YYYY-MM-DD}` syntax used in Auto Date custom formats.
const BRACE_TOKENS: TokenTable = &[
    ("YYYY", "%Y"),
    ("MMMM", "%B"),
    ("DDDD", "%j"),
    ("dddd", "%A"),
    ("MMM", "%b"),
    ("ddd", "%a"),
    ("YY", "%y"),
    ("MM", "%m"),
    ("DD", "%d"),
    ("HH", "%H"),
    ("hh", "%I"),
    ("mm", "%M"),
    ("ss", "%S"),
    ("AA", "%p"),
    ("WW", "%W"),
    ("ww", "%U"),
    ("ee", "%w"),
    ("ZZ", "%z"),
];

/// Tokens of the `{date:...}` tag formatter. Here hh is 00–23.
const FORMATTER_TOKENS: TokenTable = &[
    ("YYYY", "%Y"),
    ("MMMM", "%B"),
    ("DDDD", "%j"),
    ("dddd", "%A"),
    ("MMM", "%b"),
    ("ddd", "%a"),
    ("YY", "%y"),
    ("MM", "%m"),
    ("DD", "%d"),
    ("HH", "%H"),
    ("hh", "%H"),
    ("mm", "%M"),
    ("ss", "%S"),
    ("AA", "%p"),
];

/// Tokens of the JavaScript `formatDateTime` helper.
const JS_TOKENS: TokenTable = &[
    ("YYYY", "%Y"),
    ("MMMM", "%B"),
    ("WD3", "%a"),
    ("MMM", "%b"),
    ("WD", "%A"),
    ("YY", "%y"),
    ("MM", "%m"),
    ("DD", "%d"),
    ("HH", "%H"),
    ("hh", "%I"),
    ("mm", "%M"),
    ("ss", "%S"),
    ("M", "%-m"),
    ("D", "%-d"),
    ("H", "%-H"),
    ("h", "%-I"),
    ("A", "%p"),
    ("a", "%P"),
];

fn format_tokens(dt: &NaiveDateTime, pattern: &str, table: TokenTable, quotes: bool) -> String {
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    'outer: while i < chars.len() {
        if quotes && chars[i] == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                out.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        for (tok, code) in table {
            let t: Vec<char> = tok.chars().collect();
            if chars[i..].starts_with(&t) {
                out.push_str(&strftime(dt, code));
                i += t.len();
                continue 'outer;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Formats an Auto Date custom format: % codes, `{ }` token groups, `{{ }}` escapes and
/// a leading `|U` / `|L` for upper or lower case.
pub fn custom_format(dt: &NaiveDateTime, fmt: &str) -> String {
    let (case, body) = if let Some(rest) = fmt.strip_prefix("|U") {
        (Some(true), rest)
    } else if let Some(rest) = fmt.strip_prefix("|L") {
        (Some(false), rest)
    } else {
        (None, fmt)
    };
    // Expand { } groups first, protecting escaped braces.
    let mut expanded = String::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&'{') {
            expanded.push('{');
            i += 2;
        } else if c == '}' && chars.get(i + 1) == Some(&'}') {
            expanded.push('}');
            i += 2;
        } else if c == '{' {
            if let Some(end) = chars[i + 1..].iter().position(|&x| x == '}') {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                expanded
                    .push_str(&format_tokens(dt, &inner, BRACE_TOKENS, false).replace('%', "%%"));
                i += end + 2;
            } else {
                expanded.push(c);
                i += 1;
            }
        } else {
            expanded.push(c);
            i += 1;
        }
    }
    let s = strftime(dt, &expanded);
    match case {
        Some(true) => s.to_uppercase(),
        Some(false) => s.to_lowercase(),
        None => s,
    }
}

/// Formats a date for the `{date:pattern}` formatter (default `%Y-%m-%d_%H-%M-%S`).
pub fn formatter_date(dt: &NaiveDateTime, pattern: &str) -> String {
    if pattern.is_empty() {
        return strftime(dt, "%Y-%m-%d_%H-%M-%S");
    }
    format_tokens(dt, pattern, FORMATTER_TOKENS, true)
}

/// Formats a date for the JavaScript `formatDateTime` helper.
pub fn js_format(dt: &NaiveDateTime, pattern: &str) -> String {
    format_tokens(dt, pattern, JS_TOKENS, false)
}

/// Auto Date preset formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DatePreset {
    Dmy,
    DmyHm,
    DmyHms,
    Mdy,
    MdyHm,
    MdyHms,
    Ymd,
    YmdHm,
    YmdHms,
    Custom,
}

/// Builds the strftime string for a preset, using `seg` between parts and a 2- or 4-digit year.
pub fn preset_format(p: DatePreset, seg: &str, century: bool) -> String {
    let y = if century { "%Y" } else { "%y" };
    let seg = seg.replace('%', "%%");
    let (date, time) = match p {
        DatePreset::Dmy => (["%d", "%m", y], 0),
        DatePreset::DmyHm => (["%d", "%m", y], 2),
        DatePreset::DmyHms => (["%d", "%m", y], 3),
        DatePreset::Mdy => (["%m", "%d", y], 0),
        DatePreset::MdyHm => (["%m", "%d", y], 2),
        DatePreset::MdyHms => (["%m", "%d", y], 3),
        DatePreset::Ymd => ([y, "%m", "%d"], 0),
        DatePreset::YmdHm => ([y, "%m", "%d"], 2),
        DatePreset::YmdHms => ([y, "%m", "%d"], 3),
        DatePreset::Custom => return String::new(),
    };
    let mut parts: Vec<&str> = date.to_vec();
    parts.extend(["%H", "%M", "%S"].iter().take(time));
    parts.join(&seg)
}

// ---------------------------------------------------------------- reformat dates in names

/// Turns a strftime input format into a regex that finds candidate text.
fn format_to_regex(fmt: &str) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.next() {
                Some('Y') => out.push_str(r"\d{4}"),
                Some('y') => out.push_str(r"\d{2}"),
                Some('m') | Some('d') | Some('H') | Some('M') | Some('S') | Some('I') => {
                    out.push_str(r"\d{1,2}")
                }
                Some('j') => out.push_str(r"\d{1,3}"),
                Some('b') | Some('a') => out.push_str(r"[A-Za-z]{3}"),
                Some('B') | Some('A') => out.push_str(r"[A-Za-z]{3,9}"),
                Some('p') => out.push_str(r"[AaPp][Mm]"),
                Some(o) => out.push_str(&fancy_regex::escape(&o.to_string())),
                None => {}
            }
        } else {
            out.push_str(&fancy_regex::escape(&c.to_string()));
        }
    }
    out
}

fn try_parse(text: &str, fmt: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(text, fmt) {
        return Some(dt);
    }
    NaiveDate::parse_from_str(text, fmt)
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
}

const AUTO_FORMATS: &[&str] = &[
    "%Y-%m-%d", "%Y_%m_%d", "%Y.%m.%d", "%Y %m %d", "%Y%m%d", "%d-%m-%Y", "%d_%m_%Y", "%d.%m.%Y",
    "%d %m %Y", "%d %B %Y", "%d %b %Y", "%B %d %Y", "%b %d %Y", "%d-%b-%Y", "%d%b%Y", "%m-%d-%Y",
    "%m.%d.%Y",
];

/// Finds a date in `name` and rewrites it. `spec` is an output format, or
/// `input>output` to describe an unusual input format. Default output is `%Y-%m-%d`.
pub fn reformat_date(name: &str, spec: &str) -> String {
    let (inputs, output): (Vec<String>, String) = match spec.split_once('>') {
        Some((i, o)) => (vec![i.to_string()], o.to_string()),
        None => (
            AUTO_FORMATS.iter().map(|s| s.to_string()).collect(),
            if spec.is_empty() {
                "%Y-%m-%d".into()
            } else {
                spec.to_string()
            },
        ),
    };
    for fmt in &inputs {
        let Ok(re) = fancy_regex::Regex::new(&format!(r"(?<!\d){}(?!\d)", format_to_regex(fmt)))
        else {
            continue;
        };
        let mut search_from = 0;
        while let Ok(Some(m)) = re.find_from_pos(name, search_from) {
            if let Some(dt) = try_parse(m.as_str(), fmt) {
                if dt.year() >= 1000 || fmt.contains("%y") {
                    let formatted = strftime(&dt, &output);
                    return format!("{}{}{}", &name[..m.start()], formatted, &name[m.end()..]);
                }
            }
            search_from = m.start() + name[m.start()..].chars().next().map_or(1, char::len_utf8);
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2024, 6, 14)
            .unwrap()
            .and_hms_opt(9, 5, 7)
            .unwrap()
    }

    #[test]
    fn custom_formats_follow_bru() {
        assert_eq!(custom_format(&dt(), "%d-%b-%Y"), "14-Jun-2024");
        assert_eq!(custom_format(&dt(), "|U%d-%b-%Y"), "14-JUN-2024");
        assert_eq!(custom_format(&dt(), "|L%d-%b-%Y"), "14-jun-2024");
        assert_eq!(
            custom_format(&dt(), "Report_{YYYY-MM-DD}_at_{HH-mm-ss}"),
            "Report_2024-06-14_at_09-05-07"
        );
        assert_eq!(custom_format(&dt(), "{{x}} %%"), "{x} %");
        assert_eq!(custom_format(&dt(), "%Q bad %Y"), "%Q bad 2024");
    }

    #[test]
    fn formatter_and_js_patterns() {
        assert_eq!(formatter_date(&dt(), "YYYYMMDD"), "20240614");
        assert_eq!(
            formatter_date(&dt(), "'Shot on 'YYYY-MM-DD"),
            "Shot on 2024-06-14"
        );
        assert_eq!(formatter_date(&dt(), ""), "2024-06-14_09-05-07");
        assert_eq!(js_format(&dt(), "YYYY_MMMM_D"), "2024_June_14");
        assert_eq!(js_format(&dt(), "WD3 hh A"), "Fri 09 AM");
    }

    #[test]
    fn presets() {
        assert_eq!(preset_format(DatePreset::Dmy, "-", false), "%d-%m-%y");
        assert_eq!(preset_format(DatePreset::YmdHms, "", true), "%Y%m%d%H%M%S");
    }

    #[test]
    fn reformat_dates_in_names() {
        assert_eq!(
            reformat_date("Trip 14.06.2024 Rome", ""),
            "Trip 2024-06-14 Rome"
        );
        assert_eq!(
            reformat_date("scan_20240614", "%d %B %Y"),
            "scan_14 June 2024"
        );
        assert_eq!(
            reformat_date("x 01+04+1999 y", "%d+%m+%Y>%d %m %Y"),
            "x 01 04 1999 y"
        );
        assert_eq!(reformat_date("no date here 123", ""), "no date here 123");
    }
}
