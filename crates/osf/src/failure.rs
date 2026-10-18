//! Names why a reviewer's command failed, in osf's own words.
//!
//! A reviewer's output is untrusted text and can hold a credential, so it
//! never reaches osf's output, the journal, SARIF or a posted review. This
//! module reads it and hands back only a [`Category`], a fixed value whose
//! wording osf owns.
//!
//! Coverage: the category comes from the child's standard output and
//! standard error together. A JSON object or JSON line is read by its
//! fields first (`statusCode`, `status`, `api_error_status`, `error.type`,
//! `error.code`, `code`). Plain text patterns are the last resort. The
//! classifier is independent of the harness, so every agent in
//! [`crate::agents::AGENTS`] is read the same way. It does not cover a
//! failure whose text matches none of the patterns: that is
//! [`Category::Unknown`], which means "no pattern matched", never "nothing
//! failed".

use serde_json::Value;

/// Why a reviewer failed, as far as its output says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    SchemaRejected,
    Credential,
    ModelUnavailable,
    Quota,
    Unreachable,
    CommandLine,
    Permission,
    Unknown,
}

impl Category {
    /// osf's own wording for the category, safe to print.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        match self {
            Category::SchemaRejected => "the harness rejected the answer schema",
            Category::Credential => "the provider rejected the credential (HTTP 401 or 403)",
            Category::ModelUnavailable => "the model is unavailable to this credential",
            Category::Quota => "the provider reports a quota or rate limit",
            Category::Unreachable => "the provider could not be reached",
            Category::CommandLine => "the harness rejected its command line",
            Category::Permission => "a home or file permission was refused",
            Category::Unknown => "no known failure pattern in its output",
        }
    }
}

const TEXT_PATTERNS: &[(Category, &[&str])] = &[
    (
        Category::SchemaRejected,
        &[
            "not a valid json schema",
            "invalid_json_schema",
            "invalid schema for response_format",
            "invalid output schema",
            "output schema is invalid",
            "output schema error",
            "output schema rejected",
        ],
    ),
    (
        Category::Credential,
        &[
            "invalid api key",
            "incorrect api key",
            "invalid bearer token",
            "missing authentication",
            "user not found",
            "authentication_error",
            "invalid x-api-key",
        ],
    ),
    (
        Category::ModelUnavailable,
        &[
            "model not found",
            "model_not_found",
            "unknown model",
            "model does not exist",
            "do not have access to the model",
            "does not have access to model",
        ],
    ),
    (
        Category::Quota,
        &[
            "insufficient_quota",
            "insufficient quota",
            "insufficient credits",
            "quota exceeded",
            "exceeded your current quota",
            "exceeded your quota",
            "out of credits",
            "no credits",
            "rate limit",
            "rate_limit",
            "billing hard limit",
            "billing_not_active",
            "payment required",
        ],
    ),
    (
        Category::Unreachable,
        &[
            "enotfound",
            "econnrefused",
            "econnreset",
            "etimedout",
            "cannot connect",
            "unable to connect",
            "getaddrinfo",
            "dns lookup failed",
            "dns resolution failed",
            "tls handshake",
            "tls error",
            "ssl error",
            "certificate verify failed",
            "timed out connecting",
            "connection refused",
        ],
    ),
    (
        Category::CommandLine,
        &[
            "unknown option",
            "unknown flag",
            "unknown argument",
            "unexpected argument",
            "unrecognized option",
            "unrecognized arguments",
            "usage:",
        ],
    ),
    (
        Category::Permission,
        &[
            "eacces",
            "eperm",
            "permission denied",
            "read-only file system",
            "erofs",
        ],
    ),
];

/// The category `stdout` and `stderr` point to. The first category in
/// [`Category`] order that either source supports wins, so the answer does
/// not depend on which stream held the text.
#[must_use]
pub fn classify(stdout: &str, stderr: &str) -> Category {
    diagnose(stdout, stderr).0
}

/// [`classify`], and the basis for the category: a short value from a fixed
/// vocabulary, never text the provider wrote. It is one of the lowercase
/// phrases in `TEXT_PATTERNS`, `http status <n>`, `json status <n>`,
/// `json label <label>` where the label is one `from_label` knows, or
/// `json errno <code>`. `None` when the category is [`Category::Unknown`].
#[must_use]
pub fn diagnose(stdout: &str, stderr: &str) -> (Category, Option<String>) {
    let mut found: Option<(Category, String)> = None;
    for text in [stdout, stderr] {
        for value in json_values(text) {
            if let Some(code) = filesystem_errno(&value, 0) {
                return (Category::Permission, Some(format!("json errno {code}")));
            }
            if let Some(hit) = from_json(&value, 0) {
                found = Some(found.map_or(hit.clone(), |f| f.min(hit)));
            }
        }
    }
    if let Some((category, basis)) = found {
        return (category, Some(basis));
    }
    let lower = format!("{stdout}\n{stderr}").to_lowercase();
    let mut found: Option<(Category, String)> = None;
    for (category, needles) in TEXT_PATTERNS {
        if let Some(n) = needles.iter().find(|n| matches_text(&lower, n)) {
            let hit = (*category, (*n).to_string());
            found = Some(found.map_or(hit.clone(), |f| f.min(hit)));
        }
    }
    // An explicit filesystem error code or message outranks a word that only
    // appears in a path or a name; a rejected schema still wins.
    let explicit_permission = [
        "eacces",
        "eperm",
        "erofs",
        "permission denied",
        "read-only file system",
    ]
    .iter()
    .find(|n| matches_text(&lower, n));
    if let Some(n) = explicit_permission {
        if found
            .as_ref()
            .is_none_or(|(c, _)| *c != Category::SchemaRejected)
        {
            found = Some((Category::Permission, (*n).to_string()));
        }
    }
    // An HTTP status, read with its context, is the provider's own answer.
    if let Some(code) = status_in(&lower, &[401, 403]) {
        let hit = (Category::Credential, format!("http status {code}"));
        found = Some(found.map_or(hit.clone(), |f| f.min(hit)));
    }
    if let Some(code) = status_in(&lower, &[402, 429]) {
        let hit = (Category::Quota, format!("http status {code}"));
        found = Some(found.map_or(hit.clone(), |f| f.min(hit)));
    }
    match found {
        Some((category, basis)) => (category, Some(basis)),
        None => (Category::Unknown, None),
    }
}

/// How many of the last lines of a failed attempt's output are kept.
pub const TAIL_LINES: usize = 40;

/// The longest a kept line may be, in characters. Longer lines are cut after
/// redaction, so a cut can never split a secret.
pub const TAIL_LINE_CHARS: usize = 2000;

/// What a failed attempt's output says about its cause, before redaction.
/// `tail` is raw reviewer text: it must pass through the job's secret
/// redaction before it is saved anywhere, and it never reaches a log, the
/// journal or a posted review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cause {
    /// What made [`diagnose`] choose its category, from a fixed vocabulary.
    pub basis: Option<String>,
    /// The last [`TAIL_LINES`] lines of standard output followed by standard error.
    pub tail: String,
}

/// The [`Cause`] of a failure whose output was `stdout` and `stderr`. The two
/// streams are read separately, so the tail is the lines of standard output
/// followed by the lines of standard error, not their true interleaving.
#[must_use]
pub fn cause(stdout: &str, stderr: &str) -> Cause {
    let total = stdout.lines().count() + stderr.lines().count();
    let kept: Vec<&str> = stdout
        .lines()
        .chain(stderr.lines())
        .skip(total.saturating_sub(TAIL_LINES))
        .collect();
    Cause {
        basis: diagnose(stdout, stderr).1,
        tail: kept.join("\n"),
    }
}

/// `text` with every line cut to [`TAIL_LINE_CHARS`] characters.
#[must_use]
pub fn cap_line_lengths(text: &str) -> String {
    text.lines()
        .map(|line| match line.char_indices().nth(TAIL_LINE_CHARS) {
            Some((end, _)) => format!("{} [line cut]", &line[..end]),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The error codes a system reports. Each counts only as a whole token
/// outside path-like text: at the start of a message, or after a space or a
/// bracket, and before a space, a colon or a closing bracket.
const ERRNO_CODES: &[&str] = &[
    "eacces",
    "eperm",
    "erofs",
    "enotfound",
    "econnrefused",
    "econnreset",
    "etimedout",
];

/// Whether `text` holds `needle`, reading an error code as a token and any
/// other phrase as a plain match.
fn matches_text(text: &str, needle: &str) -> bool {
    if !ERRNO_CODES.contains(&needle) {
        return has_token(text, needle);
    }
    text.match_indices(needle).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + needle.len()..].chars().next();
        before.is_none_or(|c| c.is_whitespace() || "([{,:;".contains(c))
            && after.is_none_or(|c| c.is_whitespace() || ":,;)]}".contains(c))
    })
}

/// Whether `text` holds `needle`. A short needle, such as `401` or `tls`,
/// must stand alone, not sit inside a longer number or word.
fn has_token(text: &str, needle: &str) -> bool {
    if needle.len() > 3 {
        return text.contains(needle);
    }
    text.match_indices(needle).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + needle.len()..].chars().next();
        let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
        edge(before) && edge(after)
    })
}

/// The first of `codes` that `text` holds as an HTTP status: after a word such
/// as `status`, `http` or `error`, or before a reason such as `unauthorized`.
/// A number inside a path, a longer number or a count does not qualify.
fn status_in(text: &str, codes: &[u16]) -> Option<u16> {
    const BEFORE: &[&str] = &[
        "status",
        "http",
        "http/1.0",
        "http/1.1",
        "http/2",
        "error",
        "code",
        "statuscode",
        "status_code",
    ];
    const AFTER: &[&str] = &["unauthorized", "forbidden", "too many requests"];
    codes.iter().copied().find(|code| {
        let needle = code.to_string();
        text.match_indices(&needle).any(|(i, _)| {
            let before = &text[..i];
            let after = &text[i + needle.len()..];
            let edge_before = before.chars().next_back();
            let edge_after = after.chars().next();
            let standalone =
                |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && !"/.\\_-".contains(c));
            if !standalone(edge_before) || !standalone(edge_after) {
                return false;
            }
            let word = before
                .trim_end_matches(|c: char| c.is_whitespace() || ":=(\"'".contains(c))
                .rsplit(|c: char| c.is_whitespace() || "(\"'".contains(c))
                .next()
                .unwrap_or("")
                .trim_end_matches(':');
            let next = after.trim_start();
            BEFORE.contains(&word) || AFTER.iter().any(|a| next.starts_with(a))
        })
    })
}

/// The JSON values in `text`: the whole text, or each line that is JSON.
fn json_values(text: &str) -> Vec<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        return vec![v];
    }
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .collect()
}

fn from_status(code: u64, value: &Value) -> Option<(Category, String)> {
    let category = match code {
        401 | 403 => Category::Credential,
        402 | 429 => Category::Quota,
        404 if mentions_model(value) => Category::ModelUnavailable,
        _ => return None,
    };
    Some((category, format!("json status {code}")))
}

/// Whether `text` holds `word` as a whole word, not inside a longer one.
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
        edge(before) && edge(after)
    })
}

fn mentions_model(value: &Value) -> bool {
    match value {
        Value::String(s) => has_word(&s.to_lowercase(), "model"),
        Value::Array(a) => a.iter().any(mentions_model),
        Value::Object(o) => o.values().any(mentions_model),
        _ => false,
    }
}

/// The category a provider's or system's complete error label names, and the
/// label itself, which is one of the fixed names below, never other text.
/// Each label is matched whole, never as part of a longer word. A generic
/// `not_found_error` names an unavailable model only with model evidence in
/// the same object.
fn from_label(label: &str, object: &Value) -> Option<(Category, String)> {
    let l = label.to_lowercase();
    let category = match l.as_str() {
        "invalid_json_schema" => Category::SchemaRejected,
        "authentication_error" | "invalid_api_key" | "permission_error" => Category::Credential,
        "model_not_found" => Category::ModelUnavailable,
        "not_found_error" if mentions_model(object) => Category::ModelUnavailable,
        "insufficient_quota"
        | "rate_limit_error"
        | "rate_limit_exceeded"
        | "rate_limit"
        | "billing_not_active"
        | "billing_hard_limit_reached" => Category::Quota,
        "enotfound" | "econnrefused" | "econnreset" | "etimedout" => Category::Unreachable,
        "eacces" | "eperm" | "erofs" => Category::Permission,
        _ => return None,
    };
    Some((category, format!("json label {l}")))
}

/// The filesystem error code a `type` or `code` field anywhere in `value`
/// holds, which outranks a provider label beside it.
fn filesystem_errno(value: &Value, depth: usize) -> Option<&'static str> {
    if depth > 6 {
        return None;
    }
    match value {
        Value::Object(map) => map.iter().find_map(|(key, v)| {
            let own = if matches!(key.as_str(), "type" | "code") {
                v.as_str().and_then(|label| {
                    ["eacces", "eperm", "erofs"]
                        .into_iter()
                        .find(|code| label.eq_ignore_ascii_case(code))
                })
            } else {
                None
            };
            own.or_else(|| filesystem_errno(v, depth + 1))
        }),
        Value::Array(items) => items.iter().find_map(|v| filesystem_errno(v, depth + 1)),
        _ => None,
    }
}

/// Reads the status and label fields of a JSON value, nested objects too.
fn from_json(value: &Value, depth: usize) -> Option<(Category, String)> {
    if depth > 6 {
        return None;
    }
    match value {
        Value::Object(map) => {
            let mut found: Option<(Category, String)> = None;
            let mut take = |hit: Option<(Category, String)>| {
                if let Some(hit) = hit {
                    found = Some(found.take().map_or(hit.clone(), |f| f.min(hit)));
                }
            };
            for (key, v) in map {
                match key.as_str() {
                    "statusCode" | "status" | "status_code" | "api_error_status" => {
                        let code = v
                            .as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()));
                        take(code.and_then(|c| from_status(c, value)));
                    }
                    "type" | "code" => {
                        take(v.as_str().and_then(|label| from_label(label, value)));
                    }
                    _ => {}
                }
                if v.is_object() || v.is_array() {
                    take(from_json(v, depth + 1));
                }
            }
            found
        }
        Value::Array(items) => items.iter().filter_map(|v| from_json(v, depth + 1)).min(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basis_names_the_pattern_or_status_that_chose_the_category() {
        let cases: &[(&str, &str, Category, &str)] = &[
            (
                "",
                "ERROR: 429 Too Many Requests",
                Category::Quota,
                "http status 429",
            ),
            (
                "",
                "Incorrect API key provided: sk-FAKE",
                Category::Credential,
                "incorrect api key",
            ),
            (
                "",
                "error: quota exceeded",
                Category::Quota,
                "quota exceeded",
            ),
            (
                r#"{"error":{"statusCode":401}}"#,
                "",
                Category::Credential,
                "json status 401",
            ),
            (
                r#"{"error":{"type":"rate_limit_error"}}"#,
                "",
                Category::Quota,
                "json label rate_limit_error",
            ),
            (
                r#"{"error":{"code":"EACCES"}}"#,
                "",
                Category::Permission,
                "json errno eacces",
            ),
            (
                "",
                "Error: Permission denied while opening /out",
                Category::Permission,
                "permission denied",
            ),
        ];
        for (stdout, stderr, category, basis) in cases {
            assert_eq!(
                diagnose(stdout, stderr),
                (*category, Some((*basis).to_string())),
                "{stdout} {stderr}"
            );
        }
        assert_eq!(diagnose("all fine", ""), (Category::Unknown, None));
    }

    #[test]
    fn the_basis_never_holds_text_the_provider_wrote() {
        let (_, basis) = diagnose("", "status 429 Too Many Requests: my secret words");
        assert_eq!(basis.as_deref(), Some("http status 429"));
        let (_, basis) = diagnose(
            r#"{"error":{"type":"Rate_Limit_Error","message":"my secret words"}}"#,
            "",
        );
        assert_eq!(basis.as_deref(), Some("json label rate_limit_error"));
    }

    #[test]
    fn the_tail_is_the_last_forty_lines_of_stdout_then_stderr() {
        let numbered = |prefix: &str| {
            (1..=30)
                .map(|i| format!("{prefix} {i}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (out, err) = (numbered("out"), numbered("err"));
        let kept = cause(&out, &err).tail;
        let lines: Vec<&str> = kept.lines().collect();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines.first(), Some(&"out 21"));
        assert_eq!(lines.get(39), Some(&"err 30"));
        assert_eq!(cause("one\n", "").tail, "one");
    }

    #[test]
    fn a_long_line_is_cut_after_the_limit() {
        let capped = cap_line_lengths(&format!("{}\nshort", "x".repeat(TAIL_LINE_CHARS + 5)));
        let lines: Vec<&str> = capped.lines().collect();
        assert_eq!(
            lines.first().copied(),
            Some(format!("{} [line cut]", "x".repeat(TAIL_LINE_CHARS)).as_str())
        );
        assert_eq!(lines.get(1), Some(&"short"));
    }

    // Captured shapes, with fake keys only. Each pair is (harness, stdout,
    // stderr, expected).
    fn samples() -> Vec<(&'static str, &'static str, &'static str, Category)> {
        vec![
            ("opencode", r#"{"type":"error","error":{"message":"Missing Authentication header","statusCode":401,"url":"https://provider.invalid/v1/chat/completions"}}"#, "", Category::Credential),
            ("opencode", r#"{"type":"error","message":"No endpoints found for the model","statusCode":404}"#, "", Category::ModelUnavailable),
            ("opencode", r#"{"type":"error","error":{"message":"Rate limit exceeded","statusCode":429}}"#, "", Category::Quota),
            ("opencode", r#"{"type":"error","error":{"name":"ConnectionError","code":"ECONNREFUSED"}}"#, "", Category::Unreachable),
            ("opencode", r#"{"type":"error","error":{"data":{"message":"invalid_json_schema"}}}"#, "", Category::SchemaRejected),
            ("opencode", "error: unknown option '--bogus'\nUsage: opencode run", "", Category::CommandLine),
            ("opencode", r#"{"type":"error","error":{"code":"EACCES","message":"scandir denied"}}"#, "", Category::Permission),
            ("codex", "", "ERROR codex_models_manager::manager: failed to refresh available models: unexpected status 401 Unauthorized: Incorrect API key provided: sk-FAKE", Category::Credential),
            ("codex", "", "ERROR: stream error: 429 Too Many Requests: You exceeded your current quota", Category::Quota),
            ("codex", "", "ERROR: The model `gpt-fake` does not exist or you do not have access to the model", Category::ModelUnavailable),
            ("codex", "", "ERROR: Invalid schema for response_format 'codex_output_schema'", Category::SchemaRejected),
            ("codex", "", "ERROR: failed to connect to wss://provider.invalid/v1/responses: getaddrinfo ENOTFOUND provider.invalid", Category::Unreachable),
            ("codex", "", "error: unexpected argument '--bogus' found\n\nUsage: codex exec", Category::CommandLine),
            ("codex", "", "Error: Permission denied (os error 13) while opening /out/home", Category::Permission),
            ("claude", "", "API Error: 401 Invalid bearer token", Category::Credential),
            ("claude", r#"{"type":"result","subtype":"success","is_error":true,"api_error_status":401,"result":"Invalid API key"}"#, "", Category::Credential),
            ("claude", r#"{"type":"result","is_error":true,"api_error_status":429,"result":"x"}"#, "", Category::Quota),
            ("claude", r#"{"type":"result","is_error":true,"api_error_status":404,"result":"There's an issue with the selected model"}"#, "", Category::ModelUnavailable),
            ("claude", "", "Error: --json-schema is not a valid JSON Schema: no schema with key or ref \"https://json-schema.org/draft/2020-12/schema\"", Category::SchemaRejected),
            ("claude", "", "error: unknown option '--bogus'", Category::CommandLine),
            ("claude", "", "API Error: Unable to connect to API (ECONNRESET)", Category::Unreachable),
            ("claude", "", "EACCES: permission denied, open '.claude.json'", Category::Permission),
        ]
    }

    #[test]
    fn each_captured_shape_gets_its_category() {
        for (harness, out, err, want) in samples() {
            assert_eq!(classify(out, err), want, "{harness}: {out} {err}");
        }
    }

    // Every agent osf can drive is read the same way: the classifier is
    // harness-independent, and this runs one sample per category for each.
    #[test]
    fn every_agent_in_the_list_gets_every_category() {
        let generic = [
            ("", "HTTP 401 Unauthorized", Category::Credential),
            ("", "HTTP 429 quota exceeded", Category::Quota),
            ("", "model not found", Category::ModelUnavailable),
            ("", "not a valid JSON Schema", Category::SchemaRejected),
            (
                "",
                "connect ECONNREFUSED 10.0.0.1:443",
                Category::Unreachable,
            ),
            ("", "unrecognized arguments: --x", Category::CommandLine),
            ("", "EACCES: permission denied", Category::Permission),
            ("", "something else entirely", Category::Unknown),
        ];
        for agent in crate::agents::AGENTS {
            for (out, err, want) in generic {
                assert_eq!(classify(out, err), want, "{}: {err}", agent.name);
            }
        }
    }

    #[test]
    fn a_short_number_inside_a_longer_one_is_not_a_status() {
        assert_eq!(classify("", "took 14015 ms"), Category::Unknown);
    }

    #[test]
    fn competing_categories_follow_context_not_bare_numbers_or_words() {
        let cases = [
            (
                "EACCES: permission denied, open '/cache/401/result'",
                Category::Permission,
            ),
            (
                "permission denied writing output schema",
                Category::Permission,
            ),
            ("processed 429 items from /tmp/403/x", Category::Unknown),
            ("status 401", Category::Credential),
            ("HTTP/1.1 429 Too Many Requests", Category::Quota),
            ("401 Unauthorized", Category::Credential),
            ("error: 403", Category::Credential),
            ("version 1.401.2 built", Category::Unknown),
            (
                "invalid output schema for the answer",
                Category::SchemaRejected,
            ),
            (
                r#"{"error":{"type":"not_found_error","message":"Requested session does not exist"}}"#,
                Category::Unknown,
            ),
            (
                r#"{"error":{"type":"not_found_error","message":"model: x-1 does not exist"}}"#,
                Category::ModelUnavailable,
            ),
            (
                r#"{"error":{"type":"model_not_found"}}"#,
                Category::ModelUnavailable,
            ),
            (
                "EACCES: permission denied, open '/cache/unauthorized/config'",
                Category::Permission,
            ),
            (
                r#"{"error":{"code":"EACCES","name":"billing.json","message":"permission denied"}}"#,
                Category::Permission,
            ),
            (
                "EACCES: permission denied, open '/cache/credits.json'",
                Category::Permission,
            ),
            (
                "permission denied opening '/cache/tls/config'",
                Category::Permission,
            ),
            (
                r#"{"error":{"type":"not_found_error","message":"Requested session remodel-17 does not exist"}}"#,
                Category::Unknown,
            ),
            (
                r#"{"error":{"code":"ENOENT","message":"spawn helper ENOENT"}}"#,
                Category::Unknown,
            ),
            ("ENOENT: no such file or directory", Category::Unknown),
            (
                r#"{"error":{"type":"billing_config_error","code":"EACCES","message":"permission denied"}}"#,
                Category::Permission,
            ),
            (
                r#"{"error":{"type":"billing_config_error","message":"x"}}"#,
                Category::Unknown,
            ),
            (
                r#"{"error":{"type":"rate_limit_error","message":"slow down"}}"#,
                Category::Quota,
            ),
            (
                "ENOENT: no such file or directory, open '/cache/eacces.json'",
                Category::Unknown,
            ),
            (
                "ENOENT: no such file or directory, open '/cache/econnrefused.json'",
                Category::Unknown,
            ),
            (
                "ENOENT: no such file or directory, open 'C:\\cache\\eacces.json'",
                Category::Unknown,
            ),
            (
                "Error: connect ECONNREFUSED 127.0.0.1:443",
                Category::Unreachable,
            ),
            (
                "getaddrinfo ENOTFOUND provider.invalid",
                Category::Unreachable,
            ),
            ("Error: You exceeded your current quota", Category::Quota),
            ("insufficient_quota: add credits", Category::Quota),
            ("TLS handshake failed", Category::Unreachable),
            (
                "the credits screen rendered the tls and dns docs",
                Category::Unknown,
            ),
        ];
        for (text, want) in cases {
            assert_eq!(classify("", text), want, "{text}");
        }
    }

    #[test]
    fn a_stream_swap_does_not_change_the_category() {
        assert_eq!(classify("", "HTTP 401"), classify("HTTP 401", ""));
    }

    #[test]
    fn unknown_says_no_pattern_matched() {
        assert!(Category::Unknown
            .phrase()
            .contains("no known failure pattern"));
    }
}
