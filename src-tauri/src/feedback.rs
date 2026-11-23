//! Builds the "Report a Problem" link: the GitHub bug form with only the version and
//! operating system filled in. Nothing is sent; the link opens in the user's browser.

pub const REPOSITORY: &str = "TariqMK/Alterix";

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The bug form URL with the version and OS filled in.
pub fn report_url(version: &str, os: &str) -> String {
    format!(
        "https://github.com/{REPOSITORY}/issues/new?template=bug.yml&version={}&os={}",
        encode(version),
        encode(os)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_has_encoded_version_and_os() {
        assert_eq!(
            report_url("v0.0.0-dev", "macOS 26.1"),
            "https://github.com/TariqMK/Alterix/issues/new?template=bug.yml&version=v0.0.0-dev&os=macOS%2026.1"
        );
        assert!(report_url("v1.2.0", "Windows 11 & more").ends_with("os=Windows%2011%20%26%20more"));
    }
}
