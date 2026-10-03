//! The one marker style for every generated block in a pull request
//! description: `<!-- osf:NAME:start head=<sha> -->` ... `<!-- osf:NAME:end -->`.
//! A reader also accepts a start marker without `head=`, which the pr-lens
//! and outline writers used before, and the old status block markers, so an
//! open pull request moves to the new style on its next refresh.
//!
//! Matching is by plain text, never a regular expression, so a name that
//! holds a regex metacharacter needs no escaping.

/// The start marker for `name`, recording the commit the block was built for.
#[must_use]
pub fn start(name: &str, head: &str) -> String {
    format!("<!-- osf:{name}:start head={head} -->")
}

/// The end marker for `name`.
#[must_use]
pub fn end(name: &str) -> String {
    format!("<!-- osf:{name}:end -->")
}

/// The marker pair the status block used before the shared style.
fn legacy(name: &str) -> Option<(&'static str, &'static str)> {
    (name == "status").then_some((
        "<!-- factory:status:begin -->",
        "<!-- factory:status:end -->",
    ))
}

/// How many bytes of `text` form a start marker for `name`, when `text`
/// begins with one: the new style with or without a head, or the old one.
#[must_use]
pub fn start_len(text: &str, name: &str) -> Option<usize> {
    if let Some((old, _)) = legacy(name) {
        if text.starts_with(old) {
            return Some(old.len());
        }
    }
    let prefix = format!("<!-- osf:{name}:start");
    let rest = text.strip_prefix(prefix.as_str())?;
    if rest.starts_with(" -->") {
        return Some(prefix.len() + " -->".len());
    }
    let after_head = rest.strip_prefix(" head=")?;
    let close = after_head.find(" -->")?;
    let token = after_head.get(..close)?;
    if token.is_empty() || token.contains(char::is_whitespace) {
        return None;
    }
    Some(prefix.len() + " head=".len() + close + " -->".len())
}

/// How many bytes of `text` form an end marker for `name`, when `text`
/// begins with one: the new style or the old one.
#[must_use]
pub fn end_len(text: &str, name: &str) -> Option<usize> {
    if let Some((_, old)) = legacy(name) {
        if text.starts_with(old) {
            return Some(old.len());
        }
    }
    let marker = end(name);
    text.starts_with(marker.as_str()).then_some(marker.len())
}

/// Whether `line` is exactly a start marker for `name`, with nothing else.
#[must_use]
pub fn is_start_line(line: &str, name: &str) -> bool {
    start_len(line, name) == Some(line.len())
}

/// Whether `line` is exactly an end marker for `name`, with nothing else.
#[must_use]
pub fn is_end_line(line: &str, name: &str) -> bool {
    end_len(line, name) == Some(line.len())
}

/// Every start marker for `name` in `text`, as `(byte position, byte length)`.
#[must_use]
pub fn find_starts(text: &str, name: &str) -> Vec<(usize, usize)> {
    find_with(text, |rest| start_len(rest, name))
}

/// Every end marker for `name` in `text`, as `(byte position, byte length)`.
#[must_use]
pub fn find_ends(text: &str, name: &str) -> Vec<(usize, usize)> {
    find_with(text, |rest| end_len(rest, name))
}

fn find_with(text: &str, len_at: impl Fn(&str) -> Option<usize>) -> Vec<(usize, usize)> {
    text.match_indices("<!--")
        .filter_map(|(at, _)| {
            let rest = text.get(at..)?;
            len_at(rest).map(|len| (at, len))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_new_start_marker_carries_the_head() {
        assert_eq!(
            start("status", "abc123"),
            "<!-- osf:status:start head=abc123 -->"
        );
        assert!(is_start_line(&start("status", "abc123"), "status"));
    }

    #[test]
    fn a_start_marker_without_a_head_still_matches() {
        assert!(is_start_line("<!-- osf:pr-lens:start -->", "pr-lens"));
    }

    #[test]
    fn the_old_status_markers_match_for_status_only() {
        assert!(is_start_line("<!-- factory:status:begin -->", "status"));
        assert!(is_end_line("<!-- factory:status:end -->", "status"));
        assert!(!is_start_line("<!-- factory:status:begin -->", "outline"));
    }

    #[test]
    fn a_marker_with_trailing_text_is_not_a_marker_line() {
        assert!(!is_start_line(
            "<!-- osf:status:start head=a --> x",
            "status"
        ));
        assert!(!is_start_line("<!-- osf:status:start head= -->", "status"));
        assert!(!is_start_line(
            "<!-- osf:status:start head=a b -->",
            "status"
        ));
    }

    #[test]
    fn find_starts_reports_positions_and_lengths() {
        let text = "x <!-- osf:a:start head=1 --> y <!-- osf:a:start --> z";
        let found = find_starts(text, "a");
        assert_eq!(found.len(), 2);
        assert_eq!(
            found.first(),
            Some(&(2, "<!-- osf:a:start head=1 -->".len()))
        );
    }
}
