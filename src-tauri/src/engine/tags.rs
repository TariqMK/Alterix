//! Renaming tags such as `<name>`, `<counter>`, `<(exif:Model)>` and `<($System.ItemDate)>`,
//! with a formatter pipeline (`{upper}`, `{slice:before:-}`, `{math:+1}`, `{date:YYYY}` …).

use super::dates;
use super::text;
use chrono::NaiveDateTime;

/// A tag value: formatters treat text, numbers and dates differently.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Text(String),
    Num(f64),
    Date(NaiveDateTime),
}

impl Val {
    pub fn text(&self) -> String {
        match self {
            Val::Text(s) => s.clone(),
            Val::Num(n) => num_to_string(*n),
            Val::Date(d) => d.format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

pub fn num_to_string(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{n:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Supplies tag values for one file.
pub trait TagSource {
    /// Simple tags: name, ext, counter, size, folder, numfiles, ulid, uuid, clip, removed,
    /// randnum, randchar, rand, randname. `arg` is the text after a colon, e.g. "8" in `<rand:8>`.
    fn simple(&self, tag: &str, arg: Option<&str>) -> Option<Val>;
    /// A file property by name or label (exif:Make, PDF:Title, System.ItemDate …).
    fn property(&self, name: &str) -> Option<String>;
    /// How `$` date properties are formatted when no formatter is given (Auto Date settings).
    fn date_format(&self) -> String;
}

/// Expands every tag in `template`. Unknown simple tags are left as typed.
pub fn expand(template: &str, src: &dyn TagSource) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            if let Some(end) = find_tag_end(&chars, i + 1) {
                let inner: String = chars[i + 1..end].iter().collect();
                if let Some(v) = expand_tag(&inner, src) {
                    out.push_str(&v);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn find_tag_end(chars: &[char], start: usize) -> Option<usize> {
    let (mut brace, mut paren) = (0i32, 0i32);
    let mut quote = false;
    for (j, &c) in chars.iter().enumerate().skip(start) {
        match c {
            '\'' if brace > 0 => quote = !quote,
            '{' if !quote => brace += 1,
            '}' if !quote => brace -= 1,
            '(' if brace == 0 => paren += 1,
            ')' if brace == 0 => paren -= 1,
            '>' if brace <= 0 && paren <= 0 && !quote => return Some(j),
            '<' if brace <= 0 && paren <= 0 => return None,
            _ => {}
        }
    }
    None
}

/// Splits `{a}{b:c}` into formatter strings, returning (formatters, rest).
fn take_formatters(s: &str) -> (Vec<String>, String) {
    let chars: Vec<char> = s.chars().collect();
    let mut list = Vec::new();
    let mut i = 0;
    while i < chars.len() && chars[i] == '{' {
        let mut depth = 0;
        let mut quote = false;
        let mut j = i;
        while j < chars.len() {
            match chars[j] {
                '\'' => quote = !quote,
                '{' if !quote => depth += 1,
                '}' if !quote => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        if j >= chars.len() {
            break;
        }
        list.push(chars[i + 1..j].iter().collect());
        i = j + 1;
    }
    (list, chars[i..].iter().collect())
}

fn expand_tag(inner: &str, src: &dyn TagSource) -> Option<String> {
    if let Some(open) = inner.find('(') {
        // Property tag with optional surrounding text: <before(prop){fmt}after>
        let close = matching_paren(inner, open)?;
        let before = &inner[..open];
        let mut prop = &inner[open + 1..close];
        let (fmts, after) = take_formatters(&inner[close + 1..]);
        let kind = prop.chars().next();
        if matches!(kind, Some('$') | Some('#')) {
            prop = &prop[1..];
        }
        let raw = src.property(prop).unwrap_or_default();
        let mut val = match kind {
            Some('$') => match crate::meta::parse_date(&raw) {
                Some(d) => Val::Date(d),
                None => Val::Text(String::new()),
            },
            Some('#') => match crate::meta::as_number(&raw) {
                Some(n) => Val::Num(n),
                None => Val::Text(String::new()),
            },
            _ => Val::Text(raw),
        };
        if fmts.is_empty() {
            if let Val::Date(d) = &val {
                val = Val::Text(dates::custom_format(d, &src.date_format()));
            }
        }
        let result = apply_formatters(val, &fmts);
        if result.is_empty() {
            return Some(String::new());
        }
        return Some(format!("{before}{result}{after}"));
    }
    // Simple tag: name[:arg]{formatters}
    let split = inner.find('{').unwrap_or(inner.len());
    let head = &inner[..split];
    let (fmts, rest) = take_formatters(&inner[split..]);
    if !rest.is_empty() {
        return None;
    }
    let (name, arg) = match head.split_once(':') {
        Some((n, a)) => (n, Some(a)),
        None => (head, None),
    };
    let name_l = name.to_ascii_lowercase();
    if name_l.starts_with("hash") || name_l.is_empty() {
        return None;
    }
    let val = src.simple(&name_l, arg)?;
    Some(apply_formatters(val, &fmts))
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits formatter arguments on ':' with single quotes protecting colons.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = false;
    for c in s.chars() {
        match c {
            '\'' => quote = !quote,
            ':' if !quote => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

fn occurrence(s: Option<&String>) -> Option<isize> {
    let s = s.map(|s| s.trim().to_ascii_lowercase())?;
    let words = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
    ];
    if s == "last" {
        return Some(-1);
    }
    if let Some(i) = words.iter().position(|w| *w == s) {
        return Some(i as isize + 1);
    }
    s.parse().ok()
}

/// Parses trailing `[:n][:ci]` arguments.
fn occ_and_ci(args: &[String]) -> (isize, bool) {
    let mut n = 1;
    let mut ci = false;
    for a in args {
        let al = a.to_ascii_lowercase();
        if al == "ci" || al == "i" {
            ci = true;
        } else if let Some(o) = occurrence(Some(a)) {
            n = o;
        }
    }
    (n, ci)
}

/// Byte range of the n-th occurrence (1-based, -1 = last) of `needle` in `hay`.
fn find_nth(hay: &str, needle: &str, n: isize, ci: bool) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let re = fancy_regex::RegexBuilder::new(&fancy_regex::escape(needle))
        .case_insensitive(ci)
        .build()
        .ok()?;
    find_nth_re(hay, &re, n)
}

fn find_nth_re(hay: &str, re: &fancy_regex::Regex, n: isize) -> Option<(usize, usize)> {
    let all: Vec<(usize, usize)> = re
        .find_iter(hay)
        .filter_map(Result::ok)
        .map(|m| (m.start(), m.end()))
        .collect();
    if n < 0 {
        all.last().copied()
    } else {
        all.get((n.max(1) - 1) as usize).copied()
    }
}

/// Range (char-based) selected by a delimiter-based cut, as byte offsets into `s`.
fn delim_range(s: &str, kind: &str, args: &[String], regex: bool) -> Option<(usize, usize)> {
    let delim = args.first()?;
    let (n, ci) = occ_and_ci(args.get(1..).unwrap_or(&[]));
    let m = if regex {
        let flags_ci = ci;
        let re = fancy_regex::RegexBuilder::new(delim)
            .case_insensitive(flags_ci)
            .build()
            .ok()?;
        find_nth_re(s, &re, n)?
    } else {
        find_nth(s, delim, n, ci)?
    };
    Some(match kind {
        "before" => (0, m.0),
        "beforeinc" => (0, m.1),
        "after" => (m.1, s.len()),
        "afterinc" => (m.0, s.len()),
        _ => return None,
    })
}

fn between_range(s: &str, args: &[String], regex: bool) -> Option<(usize, usize)> {
    let left = args.first()?;
    let right = args.get(1)?;
    let ci = args
        .get(2)
        .map(|a| matches!(a.to_ascii_lowercase().as_str(), "ci" | "i"))
        .unwrap_or(false);
    let build = |p: &str| {
        let body = if regex {
            p.to_string()
        } else {
            fancy_regex::escape(p).into_owned()
        };
        fancy_regex::RegexBuilder::new(&body)
            .case_insensitive(ci)
            .build()
            .ok()
    };
    let l = build(left)?.find(s).ok()??;
    let r = build(right)?.find_from_pos(s, l.end()).ok()??;
    Some((l.end(), r.start()))
}

fn char_range(s: &str, start: i64, len: Option<i64>) -> (usize, usize) {
    let c: Vec<(usize, char)> = s.char_indices().collect();
    let n = c.len();
    let from = text::pos_to_index(start, n);
    let to = match len {
        Some(l) if l >= 0 => (from + l as usize).min(n),
        _ => n,
    };
    let byte = |i: usize| if i >= n { s.len() } else { c[i].0 };
    (byte(from), byte(to.max(from)))
}

fn number_runs(s: &str, kind: &str) -> Vec<(usize, usize)> {
    let pattern = match kind {
        "float" => r"-?\d+(?:\.\d+)?",
        "signed" => r"-?\d+",
        _ => r"\d+",
    };
    fancy_regex::Regex::new(pattern)
        .map(|re| {
            re.find_iter(s)
                .filter_map(Result::ok)
                .map(|m| (m.start(), m.end()))
                .collect()
        })
        .unwrap_or_default()
}

fn smart_title(s: &str) -> String {
    let spaced = s.replace('_', " ");
    text::change_case(&spaced, text::CaseMode::TitleEnhanced, "<ic>")
}

fn group_digits(int_part: &str, sep: &str) -> String {
    let (sign, digits) = match int_part.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", int_part),
    };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push_str(sep);
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

fn zero_pattern(n: f64, pat: &str) -> String {
    let (ip, fp) = match pat.split_once('.') {
        Some((a, b)) => (a.len(), b.len()),
        None => (pat.len(), 0),
    };
    let body = format!("{:.*}", fp, n.abs());
    let (i, f) = match body.split_once('.') {
        Some((a, b)) => (a.to_string(), Some(b.to_string())),
        None => (body.clone(), None),
    };
    let i = format!("{i:0>ip$}");
    let sign = if n < 0.0 { "-" } else { "" };
    match f {
        Some(f) => format!("{sign}{i}.{f}"),
        None => format!("{sign}{i}"),
    }
}

fn to_num(v: &Val) -> f64 {
    match v {
        Val::Num(n) => *n,
        Val::Text(s) => crate::meta::as_number(s).unwrap_or(0.0),
        Val::Date(_) => 0.0,
    }
}

fn is_date_pattern(f: &str) -> bool {
    !f.is_empty()
        && f.contains(|c| "YMDhms".contains(c))
        && f.chars()
            .all(|c| "YMDdHhmsA-_ ./:'".contains(c) || c.is_alphabetic())
}

/// Runs a formatter pipeline on `val` and returns the final text.
pub fn apply_formatters(mut val: Val, fmts: &[String]) -> String {
    // Each slice frame keeps the text around the focused part.
    let mut frames: Vec<(String, String)> = Vec::new();
    for f in fmts {
        let (name, rest) = match f.split_once(':') {
            Some((n, r)) => (n.trim().to_ascii_lowercase(), Some(r)),
            None => (f.trim().to_ascii_lowercase(), None),
        };
        let args = rest.map(split_args).unwrap_or_default();
        let s = val.text();
        let set_text = |t: String| Val::Text(t);
        val = match name.as_str() {
            "upper" => set_text(s.to_uppercase()),
            "lower" => set_text(s.to_lowercase()),
            "titlecase" | "title" => set_text(smart_title(&s)),
            "trim" => set_text(s.trim().to_string()),
            "default" => {
                if s.is_empty() {
                    set_text(args.join(":"))
                } else {
                    val
                }
            }
            "len" => Val::Num(s.chars().count() as f64),
            "nodigits" => set_text(s.chars().filter(|c| !c.is_ascii_digit()).collect()),
            "substr" => {
                let start = args.first().and_then(|a| a.parse().ok()).unwrap_or(1);
                let len = args.get(1).and_then(|a| a.parse().ok());
                let (a, b) = char_range(&s, start, len);
                set_text(s[a..b].to_string())
            }
            "rsubstr" => {
                let c: Vec<char> = s.chars().collect();
                let n = c.len() as i64;
                let (end, len) = match (
                    args.first().and_then(|a| a.parse::<i64>().ok()),
                    args.get(1).and_then(|a| a.parse::<i64>().ok()),
                ) {
                    (Some(anchor), Some(len)) => (n - anchor + 1, len),
                    (Some(len), None) => (n, len),
                    _ => (n, n),
                };
                let end = end.clamp(0, n);
                let start = (end - len).clamp(0, n);
                set_text(c[start as usize..end as usize].iter().collect())
            }
            "before" | "after" | "beforeinc" | "afterinc" => {
                match delim_range(&s, &name, &args, false) {
                    Some((a, b)) => set_text(s[a..b].to_string()),
                    None => val,
                }
            }
            "slice" | "slicere" => {
                let regex = name == "slicere";
                let kind = args
                    .first()
                    .map(|a| a.to_ascii_lowercase())
                    .unwrap_or_default();
                let range = match kind.as_str() {
                    "before" | "after" | "beforeinc" | "afterinc" => {
                        delim_range(&s, &kind, &args[1..], regex)
                    }
                    "between" => between_range(&s, &args[1..], regex),
                    _ if !regex => {
                        let start = kind.parse().unwrap_or(1);
                        let len = args.get(1).and_then(|a| a.parse().ok());
                        Some(char_range(&s, start, len))
                    }
                    _ => None,
                };
                match range {
                    Some((a, b)) => {
                        frames.push((s[..a].to_string(), s[b..].to_string()));
                        set_text(s[a..b].to_string())
                    }
                    None => val,
                }
            }
            "slicenum" => {
                let occ = occurrence(args.first()).unwrap_or(1);
                let kind = args.get(1).map(String::as_str).unwrap_or("int");
                let runs = number_runs(&s, kind);
                let pick = if occ < 0 {
                    runs.last()
                } else {
                    runs.get((occ.max(1) - 1) as usize)
                };
                match pick {
                    Some(&(a, b)) => {
                        frames.push((s[..a].to_string(), s[b..].to_string()));
                        let digits = &s[a..b];
                        // Keep leading-zero width information by passing text; math converts.
                        Val::Text(digits.to_string())
                    }
                    None => val,
                }
            }
            "replace" => {
                let from = args.first().cloned().unwrap_or_default();
                let to = args.get(1).cloned().unwrap_or_default();
                let ci = args
                    .get(2)
                    .map(|a| matches!(a.to_ascii_lowercase().as_str(), "ci" | "i"))
                    .unwrap_or(false);
                if from.is_empty() {
                    val
                } else {
                    let re = fancy_regex::RegexBuilder::new(&fancy_regex::escape(&from))
                        .case_insensitive(ci)
                        .build();
                    match re {
                        Ok(re) => {
                            set_text(re.replace_all(&s, fancy_regex::NoExpand(&to)).into_owned())
                        }
                        Err(_) => val,
                    }
                }
            }
            "set" => set_text(args.join(":")),
            "int" => Val::Num(to_num(&val).trunc()),
            "num" => {
                let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
                Val::Num(digits.parse().unwrap_or(0.0))
            }
            "math" => {
                let x = to_num(&val).trunc() as i64;
                let (op, k) = match args.as_slice() {
                    [one] => {
                        let one = one.trim();
                        match one.chars().next() {
                            Some(c @ ('+' | '-' | '*' | '/' | '^')) => {
                                (c.to_string(), one[1..].to_string())
                            }
                            _ => ("+".into(), one.to_string()),
                        }
                    }
                    [op, k, ..] => (op.to_ascii_lowercase(), k.clone()),
                    [] => ("+".into(), "0".into()),
                };
                let k: i64 = k.trim().parse().unwrap_or(0);
                let r = match op.as_str() {
                    "+" | "add" => x.saturating_add(k),
                    "-" | "sub" => x.saturating_sub(k),
                    "*" | "mul" => x.saturating_mul(k),
                    "/" | "div" => {
                        if k == 0 {
                            x
                        } else {
                            x / k
                        }
                    }
                    "^" | "pow" => x.saturating_pow(k.clamp(0, 63) as u32),
                    _ => x,
                };
                Val::Num(r as f64)
            }
            "pad" => {
                let w: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
                let ch = args.get(1).and_then(|a| a.chars().next()).unwrap_or(' ');
                let n = s.chars().count();
                set_text(format!(
                    "{}{}",
                    ch.to_string().repeat(w.saturating_sub(n)),
                    s
                ))
            }
            "pad0" => {
                let w: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
                let n = to_num(&val).trunc() as i64;
                let digits = n.unsigned_abs().to_string();
                let width = w;
                set_text(format!(
                    "{}{:0>width$}",
                    if n < 0 { "-" } else { "" },
                    digits
                ))
            }
            "padnum" => {
                let w: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
                let fill = args.get(1).and_then(|a| a.chars().next()).unwrap_or('0');
                match number_runs(&s, "int").first() {
                    Some(&(a, b)) => {
                        let d = &s[a..b];
                        let padded = format!(
                            "{}{}",
                            fill.to_string().repeat(w.saturating_sub(d.len())),
                            d
                        );
                        set_text(format!("{}{}{}", &s[..a], padded, &s[b..]))
                    }
                    None => val,
                }
            }
            "fixed" => {
                let p: usize = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
                set_text(format!("{:.*}", p, to_num(&val)))
            }
            "group" => {
                let sep = args.first().cloned().unwrap_or_else(|| ",".into());
                let n = to_num(&val);
                let t = num_to_string(n);
                let (i, f) = match t.split_once('.') {
                    Some((a, b)) => (a.to_string(), Some(b.to_string())),
                    None => (t.clone(), None),
                };
                let g = group_digits(&i, &sep);
                set_text(match f {
                    Some(f) => format!("{g}.{f}"),
                    None => g,
                })
            }
            "z" => set_text(zero_pattern(
                to_num(&val),
                args.first().map(String::as_str).unwrap_or("0"),
            )),
            "date" => match &val {
                Val::Date(d) => set_text(dates::formatter_date(d, &args.join(":"))),
                _ => match crate::meta::parse_date(&s) {
                    Some(d) => set_text(dates::formatter_date(&d, &args.join(":"))),
                    None => val,
                },
            },
            _ => {
                // A bare pattern on a date value, e.g. {YYYYMMDD}.
                let pattern = f.as_str();
                match &val {
                    Val::Date(d) if is_date_pattern(pattern) => {
                        set_text(dates::formatter_date(d, pattern))
                    }
                    _ => match crate::meta::parse_date(&s) {
                        Some(d) if is_date_pattern(pattern) => {
                            set_text(dates::formatter_date(&d, pattern))
                        }
                        _ => val,
                    },
                }
            }
        };
    }
    let mut out = val.text();
    while let Some((pre, post)) = frames.pop() {
        out = format!("{pre}{out}{post}");
    }
    out
}

/// Expands the short Add codes: %a %c %e %f %xb %ma %mo (EXIF), %r %l %t %k (ID3)
/// and %z (file size).
pub fn expand_legacy(s: &str, src: &dyn TagSource, size_text: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let codes: &[(&str, &str)] = &[
        ("%xb", "exif:ExposureBiasValue"),
        ("%ma", "exif:Make"),
        ("%mo", "exif:Model"),
        ("%a", "exif:FNumber"),
        ("%c", "exif:UserComment"),
        ("%e", "exif:ExposureTime"),
        ("%f", "exif:FocalLength"),
        ("%r", "audio:Artist"),
        ("%l", "audio:Album"),
        ("%t", "audio:Title"),
        ("%k", "audio:Track"),
    ];
    let mut out = String::new();
    let mut rest = s;
    'scan: while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if let Some(after) = tail.strip_prefix("%z") {
            out.push_str(size_text);
            rest = after;
            continue;
        }
        for (code, prop) in codes {
            if let Some(after) = tail.strip_prefix(code) {
                out.push_str(&src.property(prop).unwrap_or_default());
                rest = after;
                continue 'scan;
            }
        }
        out.push('%');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    struct Src;
    impl TagSource for Src {
        fn simple(&self, tag: &str, arg: Option<&str>) -> Option<Val> {
            match tag {
                "name" => Some(Val::Text("holiday-photos-01".into())),
                "ext" => Some(Val::Text("jpg".into())),
                "counter" => Some(Val::Num(7.0)),
                "size" => Some(Val::Num(10485760.0)),
                "rand" => Some(Val::Text(
                    "x".repeat(arg.and_then(|a| a.parse().ok()).unwrap_or(6)),
                )),
                _ => None,
            }
        }
        fn property(&self, name: &str) -> Option<String> {
            match name {
                "exif:Model" => Some("Canon EOS 5D".into()),
                "System.ItemDate" | "exif:DateTimeOriginal" => Some("2024:11:03 14:05:06".into()),
                "Subject" => Some(String::new()),
                "ISO speed" => Some("ISO100".into()),
                _ => None,
            }
        }
        fn date_format(&self) -> String {
            "%Y%m%d".into()
        }
    }

    fn x(t: &str) -> String {
        expand(t, &Src)
    }

    #[test]
    fn simple_tags_and_unknown_tags() {
        assert_eq!(x("<name>.<ext>"), "holiday-photos-01.jpg");
        assert_eq!(x("<counter{pad0:3}>"), "007");
        assert_eq!(x("<rand:3>"), "xxx");
        assert_eq!(x("a <b> c"), "a <b> c");
    }

    #[test]
    fn property_tags_with_markers_and_surrounding_text() {
        assert_eq!(x("<(exif:Model)>"), "Canon EOS 5D");
        assert_eq!(x("<(exif:Model){after: :last}>"), "5D");
        assert_eq!(x("<($exif:DateTimeOriginal)>"), "20241103");
        assert_eq!(
            x("<($exif:DateTimeOriginal){date:YYYY-MM-DD_hh-mm-ss}>"),
            "2024-11-03_14-05-06"
        );
        assert_eq!(x("<($System.ItemDate){YYYYMMDD}>"), "20241103");
        assert_eq!(x("<(#ISO speed)>"), "100");
        assert_eq!(x("x<(Subject) ->y"), "xy");
        assert_eq!(x("<[(exif:Model)]>"), "[Canon EOS 5D]");
        assert_eq!(x("<(Subject){default:Untitled}>"), "Untitled");
    }

    #[test]
    fn formatter_pipeline() {
        let f = |v: &str, fm: &[&str]| {
            apply_formatters(
                Val::Text(v.into()),
                &fm.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            f("holiday-photos-01", &["slice:before:-", "upper"]),
            "HOLIDAY-photos-01"
        );
        assert_eq!(
            f("holiday-photos-01", &["slice:after:-", "titlecase"]),
            "holiday-Photos-01"
        );
        assert_eq!(
            f("Scene-002", &["slicenum", "math:+1", "pad0:3"]),
            "Scene-003"
        );
        assert_eq!(
            f("file_220_rtu", &["slicenum", "math:+10", "pad0:3"]),
            "file_230_rtu"
        );
        assert_eq!(f("abcdef", &["substr:2:3"]), "bcd");
        assert_eq!(f("abcdef", &["substr:-3:2"]), "de");
        assert_eq!(f("abcdef", &["rsubstr:3"]), "def");
        assert_eq!(f("abcdef", &["rsubstr:2:4"]), "bcde");
        assert_eq!(f("2024-Trip-Italy-RAW", &["before:-:2"]), "2024-Trip");
        assert_eq!(f("2024-Trip-Italy-RAW", &["after:-:2"]), "Italy-RAW");
        assert_eq!(f("2024-Trip-Italy-RAW", &["beforeinc:-:2"]), "2024-Trip-");
        assert_eq!(f("2024-Trip-Italy-RAW", &["afterinc:-:2"]), "-Italy-RAW");
        assert_eq!(
            f("Report (Draft)", &["slice:between:(:)", "set:2025"]),
            "Report (2025)"
        );
        assert_eq!(f("IMG_RAW_01", &["replace:raw:JPG:ci"]), "IMG_JPG_01");
        assert_eq!(f("set", &["set:Chapter:01"]), "Chapter:01");
        assert_eq!(f("7", &["pad:3:0"]), "007");
        assert_eq!(f("-12", &["pad0:3"]), "-012");
        assert_eq!(f("ID-7a", &["padnum:3"]), "ID-007a");
        assert_eq!(f("12.3", &["fixed:2"]), "12.30");
        assert_eq!(f("12345", &["group"]), "12,345");
        assert_eq!(f("12345", &["group:_"]), "12_345");
        assert_eq!(f("3.1", &["z:00.00"]), "03.10");
        assert_eq!(f("v2.0-beta", &["num"]), "20");
        assert_eq!(f("12.9", &["int"]), "12");
        assert_eq!(f("File-001", &["len"]), "8");
        assert_eq!(f("ABC123-45Z", &["nodigits"]), "ABC-Z");
        assert_eq!(f(" hello ", &["trim"]), "hello");
        assert_eq!(f("alpha", &["slice:1:1", "upper"]), "Alpha");
        assert_eq!(f("alpha", &["substr:1:1", "upper"]), "A");
        assert_eq!(
            f("an_example of NASA", &["titlecase"]),
            "An Example of NASA"
        );
        assert_eq!(
            f("10485760", &["num", "math:/1024", "math:/1024", "fixed:2"]),
            "10.00"
        );
    }

    #[test]
    fn date_values_format() {
        let d = NaiveDate::from_ymd_opt(2025, 8, 14)
            .unwrap()
            .and_hms_opt(13, 7, 42)
            .unwrap();
        assert_eq!(
            apply_formatters(Val::Date(d), &["date:'Shot on 'YYYY-MM-DD".into()]),
            "Shot on 2025-08-14"
        );
    }

    #[test]
    fn legacy_codes() {
        assert_eq!(
            expand_legacy("%mo - %z - 100%", &Src, "10 MB"),
            "Canon EOS 5D - 10 MB - 100%"
        );
    }
}
