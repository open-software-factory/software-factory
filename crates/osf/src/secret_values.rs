//! The secrets a job holds, and removing them from text a reviewer wrote.
//!
//! Pattern redaction finds secrets by their shape. This finds the job's own
//! secrets by their exact value, in the plain form and in the base64 and hex
//! forms a reviewer could re-write them in, so none reaches a saved answer, a
//! journal line or a posted review.

use crate::agents;
use std::fmt::Write as _;

/// A value shorter than this is not treated as a secret: removing it would damage ordinary text.
const MIN_LEN: usize = 8;

/// The variables that carry a code-host token.
const CODE_HOST_TOKENS: &[&str] = &["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN"];

/// What replaces a secret's value.
pub const MARKER: &str = "[removed: a secret this job holds]";

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The values of every provider key variable of every agent in
/// [`agents::AGENTS`] and of the code-host token variables that are set here.
#[must_use]
pub fn held() -> Vec<String> {
    let provider = agents::AGENTS
        .iter()
        .filter_map(|a| a.review.as_ref())
        .flat_map(|r| r.credential_env.iter().copied());
    let mut values: Vec<String> = provider
        .chain(CODE_HOST_TOKENS.iter().copied())
        .filter_map(|name| std::env::var(name).ok())
        .filter(|value| value.len() >= MIN_LEN)
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

/// `text` with every value in `secrets`, and its base64 and hex forms, replaced by [`MARKER`].
#[must_use]
pub fn scrub(text: &str, secrets: &[String]) -> String {
    let mut forms: Vec<String> = secrets.iter().flat_map(|s| forms_of(s)).collect();
    forms.sort_unstable_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    forms.dedup();
    let mut out = text.to_string();
    for form in &forms {
        if out.contains(form.as_str()) {
            out = out.replace(form.as_str(), MARKER);
        }
    }
    out
}

/// [`scrub`] with the secrets this process holds, from [`held`].
#[must_use]
pub fn scrub_held(text: &str) -> String {
    scrub(text, &held())
}

/// Every form of `secret` a reviewer could write it in.
fn forms_of(secret: &str) -> Vec<String> {
    let bytes = secret.as_bytes();
    let mut forms = vec![secret.to_string()];
    let hex = hex_of(bytes);
    forms.push(hex.to_uppercase());
    forms.push(hex);
    for alphabet in [STANDARD, URL_SAFE] {
        for offset in 0..3 {
            forms.extend(base64_forms(bytes, offset, alphabet));
        }
    }
    forms.retain(|form| form.len() >= MIN_LEN);
    forms
}

/// The base64 text of `bytes` when it starts `offset` bytes into a base64
/// group, keeping only the characters that do not depend on the bytes around
/// it. At offset zero the whole text, with and without padding, is kept too.
fn base64_forms(bytes: &[u8], offset: usize, alphabet: &[u8; 64]) -> Vec<String> {
    let mut input = vec![0_u8; offset];
    input.extend_from_slice(bytes);
    let encoded = encode(&input, alphabet);
    let front = [0, 2, 3].get(offset).copied().unwrap_or(0);
    let partial = (offset + bytes.len()) % 3 != 0;
    let back = usize::from(partial);
    let mut forms = Vec::new();
    if let Some(inside) = encoded.get(front..encoded.len().saturating_sub(back)) {
        forms.push(inside.to_string());
    }
    if offset == 0 && partial {
        forms.push(encoded.clone());
        let pad = if (bytes.len() % 3) == 1 { "==" } else { "=" };
        forms.push(format!("{encoded}{pad}"));
    }
    forms
}

/// `bytes` as lower-case hexadecimal.
fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// `bytes` as base64 with no padding.
fn encode(bytes: &[u8], alphabet: &[u8; 64]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk.first().copied().unwrap_or(0));
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let group = (b0 << 16) | (b1 << 8) | b2;
        let chars = chunk.len() + 1;
        for index in 0..chars {
            let shift = 18 - 6 * index;
            let value = usize::try_from((group >> shift) & 0x3f).unwrap_or(0);
            out.push(char::from(alphabet.get(value).copied().unwrap_or(b'A')));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "reviewer-key-0123456789";

    #[test]
    fn the_encoder_matches_known_base64() {
        assert_eq!(encode(b"foobar", STANDARD), "Zm9vYmFy");
        assert_eq!(encode(b"fooba", STANDARD), "Zm9vYmE");
        assert_eq!(encode(b"foob", STANDARD), "Zm9vYg");
        assert_eq!(encode(&[0xfb, 0xff], STANDARD), "+/8");
        assert_eq!(encode(&[0xfb, 0xff], URL_SAFE), "-_8");
    }

    #[test]
    fn the_plain_value_is_removed() {
        let out = scrub(&format!("the key is {KEY}, ok"), &[KEY.to_string()]);
        assert!(!out.contains(KEY), "{out}");
        assert!(out.contains(MARKER), "{out}");
    }

    #[test]
    fn the_base64_form_is_removed_at_every_alignment() {
        for prefix in ["", "a", "ab", "abc", "abcd"] {
            for suffix in ["", "x", "xy", "xyz"] {
                for alphabet in [STANDARD, URL_SAFE] {
                    let whole = format!("{prefix}{KEY}{suffix}");
                    let encoded = encode(whole.as_bytes(), alphabet);
                    let text = format!("seen: {encoded}");
                    let out = scrub(&text, &[KEY.to_string()]);
                    let start = prefix.len() * 4 / 3 + 3;
                    let end = (prefix.len() + KEY.len()) * 4 / 3 - 3;
                    let middle = encoded.get(start..end).expect("the key's own characters");
                    assert!(
                        !out.contains(middle),
                        "prefix {prefix:?} suffix {suffix:?}: {out}"
                    );
                    assert!(
                        out.contains(MARKER),
                        "prefix {prefix:?} suffix {suffix:?}: {out}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_padded_base64_form_is_removed() {
        for key in [
            "reviewer-key-01234567",
            "reviewer-key-012345678",
            "reviewer-key-0123456789",
        ] {
            let mut text = encode(key.as_bytes(), STANDARD);
            while text.len() % 4 != 0 {
                text.push('=');
            }
            let out = scrub(&format!("[{text}]"), &[key.to_string()]);
            assert_eq!(out, format!("[{MARKER}]"), "{key}");
        }
    }

    #[test]
    fn the_hex_form_is_removed_in_either_case() {
        let hex = hex_of(KEY.as_bytes());
        for form in [hex.clone(), hex.to_uppercase()] {
            let out = scrub(&format!("hex {form} end"), &[KEY.to_string()]);
            assert_eq!(out, format!("hex {MARKER} end"));
        }
    }

    #[test]
    fn a_short_value_and_ordinary_text_are_left_alone() {
        let out = scrub("nothing to see, short", &["short".to_string()]);
        assert!(out.contains("short"), "{out}");
        assert_eq!(scrub("plain text", &[KEY.to_string()]), "plain text");
    }

    #[test]
    fn held_reads_every_agents_credential_variable_and_the_code_host_token() {
        let names: Vec<&str> = agents::AGENTS
            .iter()
            .filter_map(|a| a.review.as_ref())
            .flat_map(|r| r.credential_env.iter().copied())
            .collect();
        for expected in [
            "CODEX_API_KEY",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "OPENROUTER_API_KEY",
        ] {
            assert!(names.contains(&expected), "{expected}: {names:?}");
        }
        assert!(CODE_HOST_TOKENS.contains(&"GH_TOKEN"));
    }
}
