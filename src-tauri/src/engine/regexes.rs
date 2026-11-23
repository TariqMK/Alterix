//! Regular expression renaming:
//! standard mode (a match replaces the whole name), v2 mode (unmatched text is kept,
//! /i and /g modifiers), Simple mode (%1–%9 wildcards), several expressions joined
//! with (?X), and \U \L \E \u \l case modifiers in the replacement.

use fancy_regex::{Captures, Regex, RegexBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegexMode {
    Standard,
    V2,
    Simple,
}

/// Splits on the (?X) separator; `\(?X)` keeps a literal (?X).
fn split_multi(s: &str) -> Vec<String> {
    const SEP: &str = "(?X)";
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut rest = s;
    while let Some(pos) = rest.find(SEP) {
        if pos > 0 && rest[..pos].ends_with('\\') {
            cur.push_str(&rest[..pos - 1]);
            cur.push_str(SEP);
        } else {
            cur.push_str(&rest[..pos]);
            parts.push(std::mem::take(&mut cur));
        }
        rest = &rest[pos + SEP.len()..];
    }
    cur.push_str(rest);
    parts.push(cur);
    parts
}

/// Runs every expression in `pattern` (split on (?X)) over `name`.
pub fn apply(name: &str, pattern: &str, replace: &str, mode: RegexMode) -> Result<String, String> {
    if pattern.is_empty() {
        return Ok(name.to_string());
    }
    let pats = split_multi(pattern);
    let reps = split_multi(replace);
    let mut out = name.to_string();
    for (i, p) in pats.iter().enumerate() {
        let r = reps
            .get(i)
            .or_else(|| reps.first())
            .cloned()
            .unwrap_or_default();
        out = apply_one(&out, p, &r, mode)?;
    }
    Ok(out)
}

fn apply_one(name: &str, pattern: &str, replace: &str, mode: RegexMode) -> Result<String, String> {
    match mode {
        RegexMode::Simple => {
            let re = compile(&simple_to_regex(pattern), false)?;
            let rep = simple_replacement(replace);
            Ok(match re.captures(name).map_err(err)? {
                Some(c) => expand(&rep, &c),
                None => name.to_string(),
            })
        }
        RegexMode::Standard => {
            let re = compile(pattern, false)?;
            Ok(match re.captures(name).map_err(err)? {
                Some(c) => expand(replace, &c),
                None => name.to_string(),
            })
        }
        RegexMode::V2 => {
            let (body, ci, global) = strip_modifiers(pattern);
            let re = compile(&body, ci)?;
            let mut out = String::new();
            let mut last = 0;
            let mut any = false;
            let mut pos = 0;
            while pos <= name.len() {
                let Some(c) = re.captures_from_pos(name, pos).map_err(err)? else {
                    break;
                };
                let m = c.get(0).unwrap();
                out.push_str(&name[last..m.start()]);
                out.push_str(&expand(replace, &c));
                last = m.end();
                any = true;
                if !global {
                    break;
                }
                pos = if m.end() == m.start() {
                    m.end() + name[m.end()..].chars().next().map_or(1, char::len_utf8)
                } else {
                    m.end()
                };
            }
            if !any {
                return Ok(name.to_string());
            }
            out.push_str(&name[last.min(name.len())..]);
            Ok(out)
        }
    }
}

fn err(e: fancy_regex::Error) -> String {
    format!("Regular expression error: {e}")
}

fn compile(p: &str, ci: bool) -> Result<Regex, String> {
    RegexBuilder::new(p)
        .case_insensitive(ci)
        .build()
        .map_err(|e| format!("Invalid regular expression: {e}"))
}

/// Removes trailing /i and /g modifiers (v2 only).
fn strip_modifiers(p: &str) -> (String, bool, bool) {
    let mut body = p.to_string();
    let (mut ci, mut g) = (false, false);
    loop {
        if body.ends_with("/i") && !body.ends_with("\\/i") {
            ci = true;
            body.truncate(body.len() - 2);
        } else if body.ends_with("/g") && !body.ends_with("\\/g") {
            g = true;
            body.truncate(body.len() - 2);
        } else {
            break;
        }
    }
    (body, ci, g)
}

/// Simple mode: %1–%9 match any text; everything else is literal. The pattern must
/// match the whole name.
fn simple_to_regex(p: &str) -> String {
    let mut out = String::from("^");
    let chars: Vec<char> = p.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%'
            && i + 1 < chars.len()
            && chars[i + 1].is_ascii_digit()
            && chars[i + 1] != '0'
        {
            // A tag directly followed by another tag takes one character, so
            // %1%2%3%4 splits the first three characters off.
            let next_is_tag = i + 3 < chars.len() + 1
                && chars.get(i + 2) == Some(&'%')
                && chars.get(i + 3).is_some_and(|c| c.is_ascii_digit());
            let last = i + 2 >= chars.len();
            out.push_str(if next_is_tag {
                "(.)"
            } else if last {
                "(.*)"
            } else {
                "(.*?)"
            });
            i += 2;
        } else {
            out.push_str(&fancy_regex::escape(&chars[i].to_string()));
            i += 1;
        }
    }
    out.push('$');
    out
}

fn simple_replacement(r: &str) -> String {
    // %n becomes \n; everything else (including \U etc.) passes through.
    let mut out = String::new();
    let chars: Vec<char> = r.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
            out.push('\\');
            out.push(chars[i + 1]);
            i += 2;
        } else if chars[i] == '$' {
            out.push_str("$$");
            i += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum CaseState {
    None,
    Upper,
    Lower,
}

/// Expands a replacement template: \0–\9, $0–$99, ${name}, $$, \\, plus \U \L \E \u \l.
pub fn expand(template: &str, caps: &Captures<'_, str>) -> String {
    let mut out = String::new();
    let mut state = CaseState::None;
    let mut one: Option<bool> = None; // Some(true) = next upper, Some(false) = next lower
    let push = |out: &mut String, s: &str, state: CaseState, one: &mut Option<bool>| {
        for c in s.chars() {
            let mut piece: String = match state {
                CaseState::Upper => c.to_uppercase().collect(),
                CaseState::Lower => c.to_lowercase().collect(),
                CaseState::None => c.to_string(),
            };
            if let Some(up) = one.take() {
                piece = if up {
                    piece.to_uppercase()
                } else {
                    piece.to_lowercase()
                };
            }
            out.push_str(&piece);
        }
    };
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            match n {
                'U' => state = CaseState::Upper,
                'L' => state = CaseState::Lower,
                'E' => state = CaseState::None,
                'u' => one = Some(true),
                'l' => one = Some(false),
                d if d.is_ascii_digit() => {
                    let g = caps
                        .get(d.to_digit(10).unwrap() as usize)
                        .map_or("", |m| m.as_str());
                    push(&mut out, g, state, &mut one);
                }
                other => push(&mut out, &other.to_string(), state, &mut one),
            }
            i += 2;
            continue;
        }
        if c == '$' && i + 1 < chars.len() {
            let n = chars[i + 1];
            if n == '$' {
                push(&mut out, "$", state, &mut one);
                i += 2;
                continue;
            }
            if n == '{' {
                if let Some(end) = chars[i + 2..].iter().position(|&x| x == '}') {
                    let name: String = chars[i + 2..i + 2 + end].iter().collect();
                    let g = match name.parse::<usize>() {
                        Ok(idx) => caps.get(idx),
                        Err(_) => caps.name(&name),
                    }
                    .map_or("", |m| m.as_str());
                    push(&mut out, g, state, &mut one);
                    i += 3 + end;
                    continue;
                }
            }
            if n.is_ascii_digit() {
                let mut j = i + 1;
                let mut num = String::new();
                while j < chars.len() && chars[j].is_ascii_digit() && num.len() < 2 {
                    num.push(chars[j]);
                    j += 1;
                }
                let idx: usize = num.parse().unwrap_or(0);
                let g = caps.get(idx).map_or("", |m| m.as_str());
                push(&mut out, g, state, &mut one);
                i = j;
                continue;
            }
        }
        push(&mut out, &c.to_string(), state, &mut one);
        i += 1;
    }
    out
}

/// Applies case modifiers (\U \L \E \u \l) to plain text.
pub fn case_modifiers(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let re = Regex::new("^").unwrap();
    let caps = re.captures("").unwrap().unwrap();
    expand(&s.replace('$', "$$"), &caps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use RegexMode::*;

    #[test]
    fn standard_mode_replaces_whole_name_on_match() {
        assert_eq!(
            apply(
                "Program Files",
                r"^([A-Z][a-z]*) ([A-Z][a-z]*)",
                r"\2 \1",
                Standard
            )
            .unwrap(),
            "Files Program"
        );
        assert_eq!(
            apply(
                "Program Files X",
                r"^([A-Z][a-z]*) ([A-Z][a-z]*)(.*)",
                r"\2 \1\3",
                Standard
            )
            .unwrap(),
            "Files Program X"
        );
        assert_eq!(apply("abc", r"\d+", "X", Standard).unwrap(), "abc");
        assert_eq!(apply("a1b22", r"(\d+)", "$1", Standard).unwrap(), "1");
    }

    #[test]
    fn v2_mode_keeps_unmatched_text_and_supports_modifiers() {
        assert_eq!(
            apply("Hello World", r"(\w)/g", r"\L$1", V2).unwrap(),
            "hello world"
        );
        assert_eq!(apply("a-b-c", "-", "_", V2).unwrap(), "a_b-c");
        assert_eq!(apply("a-b-c", "-/g", "_", V2).unwrap(), "a_b_c");
        assert_eq!(apply("ABC", "b/i", "x", V2).unwrap(), "AxC");
        assert_eq!(
            apply("price100", r"(?<=price)\d+", "N", V2).unwrap(),
            "priceN"
        );
    }

    #[test]
    fn simple_mode_tags() {
        assert_eq!(
            apply("abc-def", "%1-%2", "%2-%1", Simple).unwrap(),
            "def-abc"
        );
        assert_eq!(
            apply("Joe (draft)", "%1(%2)", "%2", Simple).unwrap(),
            "draft"
        );
        assert_eq!(
            apply("2020-03-12 Boston", "%1-%2-%3 %4", "%3-%2-%1 %4", Simple).unwrap(),
            "12-03-2020 Boston"
        );
        assert_eq!(
            apply("xyzname", "%1%2%3%4", "ABC%4", Simple).unwrap(),
            "ABCname"
        );
        assert_eq!(
            apply("a_b_c_d", "%1_%2_%3", "%1_%2", Simple).unwrap(),
            "a_b"
        );
        assert_eq!(apply("hello", "%1", r"\U%1", Simple).unwrap(), "HELLO");
        assert_eq!(
            apply("ab_CD", "%1_%2", r"\U%1\E_\L%2", Simple).unwrap(),
            "AB_cd"
        );
        assert_eq!(
            apply("ABCdef", "%1%2%3%4", r"\L%1%2%3\E%4", Simple).unwrap(),
            "abcdef"
        );
    }

    #[test]
    fn multiple_expressions() {
        assert_eq!(apply("SP", "(S)(?X)(P)", "A(?X)R", V2).unwrap(), "AR");
        assert_eq!(apply("SP", "(S)(?X)(P)", "A", V2).unwrap(), "AA");
        assert_eq!(split_multi(r"a\(?X)b"), vec!["a(?X)b".to_string()]);
    }

    #[test]
    fn bad_patterns_report_errors() {
        assert!(apply("a", "(", "", Standard).is_err());
    }

    #[test]
    fn plain_case_modifiers() {
        assert_eq!(case_modifiers(r"\Uabc\E-\udef"), "ABC-Def");
        assert_eq!(case_modifiers("cost $5"), "cost $5");
    }
}
