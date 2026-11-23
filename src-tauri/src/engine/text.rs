//! Text helpers shared by the rename rules: case styles, character classes, accents,
//! wildcards, Roman and Hebrew numerals and number bases. All positions are 1-based
//! character positions; negative positions count from the end.

use unicode_normalization::UnicodeNormalization;

/// Converts a 1-based position (negative = from the end, -1 = last char) into a
/// 0-based index clamped to `0..=len`.
pub fn pos_to_index(pos: i64, len: usize) -> usize {
    let len_i = len as i64;
    let idx = if pos > 0 {
        pos - 1
    } else if pos < 0 {
        len_i + pos
    } else {
        0
    };
    idx.clamp(0, len_i) as usize
}

/// Returns the characters of `s` as a vector, for position-based edits.
pub fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// Inserts `text` at 1-based `pos` (negative counts from the end; 0 or past the end appends).
pub fn insert_at(s: &str, pos: i64, text: &str) -> String {
    let c = chars(s);
    let idx = if pos == 0 {
        c.len()
    } else if pos < 0 {
        // -1 inserts before the last character.
        pos_to_index(pos, c.len())
    } else {
        pos_to_index(pos, c.len())
    };
    let mut out: String = c[..idx].iter().collect();
    out.push_str(text);
    out.extend(&c[idx..]);
    out
}

// ---------------------------------------------------------------- case

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseMode {
    Same,
    Lower,
    Upper,
    Title,
    Sentence,
    TitleEnhanced,
}

const SMALL_WORDS: &[&str] = &[
    "a", "an", "and", "as", "at", "but", "by", "en", "for", "if", "in", "nor", "of", "on", "or",
    "per", "the", "to", "v", "vs", "via",
];

/// Parsed form of the Case exceptions field.
#[derive(Default, Debug)]
struct Exceptions {
    clear_defaults: bool,
    ignore_all_caps: bool,
    roman: Option<bool>, // Some(true) = upper, Some(false) = lower
    small_words: Vec<String>,
    upper_words: Vec<String>,
    literal: Vec<String>,
}

fn parse_exceptions(spec: &str) -> Exceptions {
    let mut ex = Exceptions::default();
    for part in spec.split(':') {
        if part.is_empty() {
            continue;
        }
        match part.trim().to_ascii_lowercase().as_str() {
            "<clear>" => ex.clear_defaults = true,
            "<ic>" => ex.ignore_all_caps = true,
            "<rnlo>" => ex.roman = Some(false),
            "<rnup>" => ex.roman = Some(true),
            _ => {
                if let Some(word) = part.strip_prefix('$') {
                    if !word.is_empty() && word.chars().all(|c| !c.is_lowercase()) {
                        ex.upper_words.push(word.to_string());
                    } else {
                        ex.small_words.push(word.to_lowercase());
                    }
                } else {
                    ex.literal.push(part.to_string());
                }
            }
        }
    }
    ex
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

/// Splits `s` into alternating runs of word and non-word characters.
fn word_runs(s: &str) -> Vec<(bool, String)> {
    let mut runs: Vec<(bool, String)> = Vec::new();
    for c in s.chars() {
        let w = is_word_char(c);
        match runs.last_mut() {
            Some((kind, text)) if *kind == w => text.push(c),
            _ => runs.push((w, c.to_string())),
        }
    }
    runs
}

fn capitalise(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    let mut first = true;
    for c in word.chars() {
        if first && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            first = false;
        } else {
            if c.is_alphabetic() {
                first = false;
            }
            out.extend(c.to_lowercase());
        }
    }
    out
}

pub fn is_roman(word: &str) -> bool {
    if word.is_empty() || !word.chars().all(|c| "ivxlcdmIVXLCDM".contains(c)) {
        return false;
    }
    let up = word.to_uppercase();
    from_roman(&up).map(|n| to_roman(n) == up).unwrap_or(false)
}

/// Applies a case style plus the exceptions list.
pub fn change_case(s: &str, mode: CaseMode, exceptions: &str) -> String {
    let ex = parse_exceptions(exceptions);
    let mut out = match mode {
        CaseMode::Same => s.to_string(),
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Title => word_runs(s)
            .into_iter()
            .map(|(w, t)| if w { capitalise(&t) } else { t })
            .collect(),
        CaseMode::Sentence => sentence_case(s),
        CaseMode::TitleEnhanced => title_enhanced(s, &ex),
    };
    if let Some(upper) = ex.roman {
        out = word_runs(&out)
            .into_iter()
            .map(|(w, t)| {
                if w && is_roman(&t) {
                    if upper {
                        t.to_uppercase()
                    } else {
                        t.to_lowercase()
                    }
                } else {
                    t
                }
            })
            .collect();
    }
    for lit in &ex.literal {
        out = replace_ci_keep(&out, lit);
    }
    out
}

fn sentence_case(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut cap_next = true;
    for c in lower.chars() {
        if cap_next && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            cap_next = false;
        } else {
            out.push(c);
        }
        if c == '.' {
            cap_next = true;
        }
    }
    out
}

fn title_enhanced(s: &str, ex: &Exceptions) -> String {
    let mut small: Vec<String> = if ex.clear_defaults {
        Vec::new()
    } else {
        SMALL_WORDS.iter().map(|w| w.to_string()).collect()
    };
    small.extend(ex.small_words.iter().cloned());
    let runs = word_runs(s);
    let word_idx: Vec<usize> = runs
        .iter()
        .enumerate()
        .filter(|(_, (w, _))| *w)
        .map(|(i, _)| i)
        .collect();
    let first = word_idx.first().copied();
    let last = word_idx.last().copied();
    runs.into_iter()
        .enumerate()
        .map(|(i, (w, t))| {
            if !w {
                return t;
            }
            if ex.ignore_all_caps
                && t.chars().any(char::is_alphabetic)
                && !t.chars().any(char::is_lowercase)
            {
                return t;
            }
            if ex.upper_words.iter().any(|u| u.eq_ignore_ascii_case(&t)) {
                return t.to_uppercase();
            }
            let edge = Some(i) == first || Some(i) == last;
            if !edge && small.iter().any(|sw| sw.eq_ignore_ascii_case(&t)) {
                t.to_lowercase()
            } else {
                capitalise(&t)
            }
        })
        .collect()
}

/// Replaces every case-insensitive occurrence of `pattern` in `s` with `pattern` itself,
/// forcing that exact casing.
pub fn replace_ci_keep(s: &str, pattern: &str) -> String {
    if pattern.is_empty() {
        return s.to_string();
    }
    let re = fancy_regex::RegexBuilder::new(&fancy_regex::escape(pattern))
        .case_insensitive(true)
        .build();
    match re {
        Ok(re) => re
            .replace_all(s, fancy_regex::NoExpand(pattern))
            .into_owned(),
        Err(_) => s.to_string(),
    }
}

// ---------------------------------------------------------------- character classes

/// True for characters the Remove step's Symbols option removes.
pub fn is_symbol(c: char) -> bool {
    const ASCII: &str = "!£$%^&*()-_=+[]{};:'@#~,.<>/?\\|`¬¦\"";
    if ASCII.contains(c) {
        return true;
    }
    let u = c as u32;
    matches!(u,
        0xA1..=0xBF | 0xD7 | 0xF7          // Latin-1 Supplement symbols
        | 0x2000..=0x206F                  // General Punctuation
        | 0x2190..=0x21FF                  // Arrows
        | 0x2600..=0x26FF                  // Miscellaneous Symbols
        | 0x2700..=0x27BF                  // Dingbats
        | 0x25A0..=0x25FF                  // Geometric Shapes
        | 0x1F300..=0x1F5FF                // Misc Symbols and Pictographs
        | 0x1F600..=0x1F64F                // Emoticons
        | 0x1F680..=0x1F6FF                // Transport and Map
        | 0x1F700..=0x1F77F                // Alchemical
        | 0x1F780..=0x1F7FF                // Geometric Shapes Extended
        | 0x1F900..=0x1F9FF                // Supplemental Symbols and Pictographs
        | 0x1FA70..=0x1FAFF                // Symbols and Pictographs Extended-A
        | 0xFE0F                           // emoji variation selector
    )
}

/// Replaces accented letters with plain ones (é → e, ß → ss, Ø → O).
pub fn remove_accents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' => out.push_str("ss"),
            'ẞ' => out.push_str("SS"),
            'æ' => out.push_str("ae"),
            'Æ' => out.push_str("AE"),
            'œ' => out.push_str("oe"),
            'Œ' => out.push_str("OE"),
            'ø' => out.push('o'),
            'Ø' => out.push('O'),
            'đ' | 'ð' => out.push('d'),
            'Đ' | 'Ð' => out.push('D'),
            'ł' => out.push('l'),
            'Ł' => out.push('L'),
            'þ' => out.push_str("th"),
            'Þ' => out.push_str("TH"),
            'ı' => out.push('i'),
            _ => {
                for d in c.to_string().nfd() {
                    if !is_combining(d) {
                        out.push(d);
                    }
                }
            }
        }
    }
    out
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Removes characters named in `spec`, or matching a range condition such as
/// `>c`, `<Z` or `>c<Z`.
pub fn remove_chars(s: &str, spec: &str) -> String {
    if spec.is_empty() {
        return s.to_string();
    }
    let first = spec.chars().next().unwrap();
    if (first == '>' || first == '<') && spec.chars().count() >= 2 {
        let mut gt: Option<char> = None;
        let mut lt: Option<char> = None;
        let mut it = spec.chars().peekable();
        while let Some(op) = it.next() {
            if let Some(v) = it.next() {
                match op {
                    '>' => gt = Some(v),
                    '<' => lt = Some(v),
                    _ => return s.chars().filter(|c| !spec.contains(*c)).collect(),
                }
            }
        }
        return s
            .chars()
            .filter(|c| {
                let above = gt.map(|g| *c > g).unwrap_or(true);
                let below = lt.map(|l| *c < l).unwrap_or(true);
                !(above && below)
            })
            .collect();
    }
    s.chars().filter(|c| !spec.contains(*c)).collect()
}

/// Removes whole words (space separated in `spec`) from `s`, ignoring case.
pub fn remove_words(s: &str, spec: &str) -> String {
    let words: Vec<String> = spec.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return s.to_string();
    }
    let kept: Vec<&str> = s
        .split(' ')
        .filter(|w| w.is_empty() || !words.contains(&w.to_lowercase()))
        .collect();
    let joined = kept.join(" ");
    collapse_spaces_between(&joined, s)
}

fn collapse_spaces_between(new: &str, old: &str) -> String {
    // Removing a word leaves a doubled space; collapse only spaces we created.
    if old.contains("  ") {
        new.to_string()
    } else {
        let mut out = String::with_capacity(new.len());
        let mut prev_space = false;
        for c in new.chars() {
            if c == ' ' && prev_space {
                continue;
            }
            prev_space = c == ' ';
            out.push(c);
        }
        out.trim().to_string()
    }
}

/// Collapses runs of spaces into one space.
pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = false;
    for c in s.chars() {
        if c == ' ' {
            if !prev {
                out.push(c);
            }
            prev = true;
        } else {
            out.push(c);
            prev = false;
        }
    }
    out
}

/// Inserts a space before a capital that follows a lowercase letter or a digit.
pub fn word_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if let Some(p) = prev {
            if c.is_uppercase() && (p.is_lowercase() || p.is_ascii_digit()) {
                out.push(' ');
            }
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

// ---------------------------------------------------------------- wildcards

/// Converts a `*` / `?` wildcard pattern into a regular expression body.
/// `*` is lazy except at the very end, where it runs to the end of the text.
pub fn wildcard_to_regex(pattern: &str) -> String {
    let mut out = String::new();
    let n = pattern.chars().count();
    for (i, c) in pattern.chars().enumerate() {
        match c {
            '*' if i == n - 1 => out.push_str("(.*)"),
            '*' => out.push_str("(.*?)"),
            '?' => out.push_str("(.)"),
            _ => out.push_str(&fancy_regex::escape(&c.to_string())),
        }
    }
    out
}

/// True when `name` matches a file mask such as `*.jpg` or `IMG_??.*` (whole name).
pub fn mask_matches(mask: &str, name: &str, case_sensitive: bool) -> bool {
    let body = format!("^{}$", wildcard_to_regex(mask).replace("(.*?)", "(.*)"));
    fancy_regex::RegexBuilder::new(&body)
        .case_insensitive(!case_sensitive)
        .build()
        .ok()
        .and_then(|re| re.is_match(name).ok())
        .unwrap_or(false)
}

// ---------------------------------------------------------------- numerals

pub fn to_roman(mut n: i64) -> String {
    if n <= 0 || n >= 400_000 {
        return n.to_string();
    }
    const TABLE: &[(i64, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (v, s) in TABLE {
        while n >= *v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

pub fn from_roman(s: &str) -> Option<i64> {
    let val = |c: char| match c {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        'D' => Some(500),
        'M' => Some(1000),
        _ => None,
    };
    let vals: Option<Vec<i64>> = s.chars().map(val).collect();
    let vals = vals?;
    let mut total = 0;
    for i in 0..vals.len() {
        if i + 1 < vals.len() && vals[i] < vals[i + 1] {
            total -= vals[i];
        } else {
            total += vals[i];
        }
    }
    Some(total)
}

/// Hebrew numerals (gematria), 1–9999. 15 and 16 use the traditional forms.
pub fn to_hebrew(n: i64) -> String {
    if n <= 0 || n > 9999 {
        return n.to_string();
    }
    const ONES: [&str; 10] = ["", "א", "ב", "ג", "ד", "ה", "ו", "ז", "ח", "ט"];
    const TENS: [&str; 10] = ["", "י", "כ", "ל", "מ", "נ", "ס", "ע", "פ", "צ"];
    const HUNDREDS: [&str; 5] = ["", "ק", "ר", "ש", "ת"];
    let mut out = String::new();
    let thousands = n / 1000;
    if thousands > 0 {
        out.push_str(ONES[thousands as usize]);
        out.push('׳');
    }
    let mut h = (n % 1000) / 100;
    while h > 4 {
        out.push_str(HUNDREDS[4]);
        h -= 4;
    }
    out.push_str(HUNDREDS[h as usize]);
    let rest = n % 100;
    match rest {
        15 => out.push_str("טו"),
        16 => out.push_str("טז"),
        _ => {
            out.push_str(TENS[(rest / 10) as usize]);
            out.push_str(ONES[(rest % 10) as usize]);
        }
    }
    out
}

/// Formats `n` in base 2–36 using digits 0–9 then A–Z.
pub fn to_base(n: i64, base: u32) -> String {
    let base = base.clamp(2, 36) as i64;
    if n == 0 {
        return "0".into();
    }
    let neg = n < 0;
    let mut v = n.unsigned_abs();
    let mut digits = Vec::new();
    while v > 0 {
        let d = (v % base as u64) as u32;
        digits.push(
            std::char::from_digit(d, base as u32)
                .unwrap()
                .to_ascii_uppercase(),
        );
        v /= base as u64;
    }
    if neg {
        digits.push('-');
    }
    digits.iter().rev().collect()
}

/// Formats `n` as letters, where A = 0, B = 1 … Z = 25, BA = 26 (base 26 with letter digits).
pub fn to_letters(n: i64, upper: bool) -> String {
    let a = if upper { b'A' } else { b'a' };
    if n < 0 {
        return n.to_string();
    }
    let mut v = n as u64;
    let mut out = Vec::new();
    loop {
        out.push((a + (v % 26) as u8) as char);
        v /= 26;
        if v == 0 {
            break;
        }
    }
    out.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_handle_negative_and_overflow() {
        assert_eq!(pos_to_index(1, 5), 0);
        assert_eq!(pos_to_index(-1, 5), 4);
        assert_eq!(pos_to_index(9, 5), 5);
        assert_eq!(insert_at("abcd", 3, "X"), "abXcd");
        assert_eq!(insert_at("abcd", -1, "X"), "abcXd");
        assert_eq!(insert_at("abcd", 0, "X"), "abcdX");
    }

    #[test]
    fn title_and_sentence_follow_bru_examples() {
        assert_eq!(change_case("joe public", CaseMode::Title, ""), "Joe Public");
        assert_eq!(
            change_case("hello EVERYONE. MY name IS FreD", CaseMode::Sentence, ""),
            "Hello everyone. My name is fred"
        );
        assert_eq!(
            change_case("joe's (old)-photo", CaseMode::Title, ""),
            "Joe's (Old)-Photo"
        );
    }

    #[test]
    fn title_enhanced_keeps_small_words_low_except_edges() {
        assert_eq!(
            change_case("the lord of the rings", CaseMode::TitleEnhanced, ""),
            "The Lord of the Rings"
        );
        assert_eq!(
            change_case(
                "walk between the lines of",
                CaseMode::TitleEnhanced,
                "$between"
            ),
            "Walk between the Lines Of"
        );
        assert_eq!(
            change_case("a NASA report", CaseMode::TitleEnhanced, "<ic>"),
            "A NASA Report"
        );
        assert_eq!(
            change_case("the lord of the rings", CaseMode::TitleEnhanced, "<clear>"),
            "The Lord Of The Rings"
        );
        assert_eq!(
            change_case("my book", CaseMode::TitleEnhanced, "$BOOK"),
            "My BOOK"
        );
    }

    #[test]
    fn exceptions_force_casing() {
        assert_eq!(
            change_case("my php and asp doc", CaseMode::Title, "PHP:ASP:doc"),
            "My PHP And ASP doc"
        );
        assert_eq!(
            change_case(
                "beethoven's ninth symphony part iii",
                CaseMode::Title,
                "<rnup>"
            ),
            "Beethoven's Ninth Symphony Part III"
        );
    }

    #[test]
    fn character_removal() {
        assert_eq!(remove_chars("aQbW:c#", "QW:#"), "abc");
        assert_eq!(remove_chars("abcxyz", ">c"), "abc");
        assert_eq!(remove_chars("ABCabc", "<Z"), "abc");
        assert_eq!(
            remove_accents("Crème brûlée à Straße"),
            "Creme brulee a Strasse"
        );
        assert_eq!(remove_words("the big red dog", "BIG dog"), "the red");
        assert_eq!(collapse_spaces("a  b   c"), "a b c");
        assert!(is_symbol('£') && is_symbol('😀') && !is_symbol('a') && !is_symbol(' '));
    }

    #[test]
    fn word_space_splits_camel_case() {
        assert_eq!(
            word_space("MyHoliday Photographs"),
            "My Holiday Photographs"
        );
        assert_eq!(word_space("Day2Trip"), "Day2 Trip");
    }

    #[test]
    fn masks() {
        assert!(mask_matches("*.jpg", "Photo.JPG", false));
        assert!(!mask_matches("*.jpg", "Photo.JPG", true));
        assert!(mask_matches("IMG_??.*", "IMG_01.png", false));
        assert!(!mask_matches("IMG_??.*", "IMG_001.png", false));
    }

    #[test]
    fn numerals_and_bases() {
        assert_eq!(to_roman(1994), "MCMXCIV");
        assert_eq!(from_roman("MCMXCIV"), Some(1994));
        assert!(is_roman("iii") && is_roman("MIX") && !is_roman("IIII") && !is_roman("mxm"));
        assert_eq!(to_hebrew(15), "טו");
        assert_eq!(to_hebrew(5), "ה");
        assert_eq!(to_base(26, 16), "1A");
        assert_eq!(to_base(5, 2), "101");
        assert_eq!(to_letters(0, true), "A");
        assert_eq!(to_letters(27, false), "bb");
    }
}
