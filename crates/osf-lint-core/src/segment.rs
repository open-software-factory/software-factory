//! Split Markdown-ish text into sentences, paragraphs and a whole-document
//! unit, each with a line number and a byte span. A rule or an analyser
//! reads whichever grain its scope asks for; the shape is the same for all.

use regex::Regex;
use std::ops::Range;
use std::sync::OnceLock;

/// One grain of text: a sentence, a paragraph, or a whole document.
/// Any rule or analyser, regular expression or model, takes this as input
/// and reports a location the same way.
#[derive(Clone, Debug)]
pub struct TextUnit {
    pub text: String,
    pub span: Range<usize>,
    pub line: usize,
    /// A table cell: names and phrases are checked, length is not.
    pub in_table: bool,
    /// A heading: its first word is never a name candidate.
    pub is_heading: bool,
}

impl TextUnit {
    /// Words after inline code and links are reduced, for counting and names.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        reduce_inline(&self.text)
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }
}

pub struct Doc {
    pub sentences: Vec<TextUnit>,
    pub paragraphs: Vec<TextUnit>,
    pub whole: TextUnit,
    pub headings: Vec<usize>,
    pub word_count: usize,
}

struct Para {
    line: usize,
    text: String,
    /// (byte offset in `text`, source line) for every joined line.
    line_starts: Vec<(usize, usize)>,
    in_table: bool,
    is_heading: bool,
}

impl Para {
    fn new(line: usize, text: &str) -> Self {
        Para {
            line,
            text: text.to_string(),
            line_starts: vec![(0, line)],
            in_table: false,
            is_heading: false,
        }
    }

    fn cell(line: usize, text: &str) -> Self {
        Para {
            in_table: true,
            ..Para::new(line, text)
        }
    }

    fn heading(line: usize, text: &str) -> Self {
        Para {
            is_heading: true,
            ..Para::new(line, text)
        }
    }

    fn append(&mut self, line: usize, text: &str) {
        self.text.push(' ');
        self.line_starts.push((self.text.len(), line));
        self.text.push_str(text);
    }

    fn line_at(&self, offset: usize) -> usize {
        self.line_starts
            .iter()
            .take_while(|(o, _)| *o <= offset)
            .last()
            .map_or(self.line, |(_, l)| *l)
    }
}

/// One line of input, classified. The parser is a fold over these.
enum Line<'a> {
    Fence,
    CommentOpen,
    CommentClose,
    Blank,
    Table(Vec<&'a str>),
    Heading(&'a str),
    Item(&'a str),
    Prose(&'a str),
}

fn classify(trimmed: &str) -> Line<'_> {
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return Line::Fence;
    }
    if trimmed.starts_with("<!--") {
        return if trimmed.contains("-->") {
            Line::Blank
        } else {
            Line::CommentOpen
        };
    }
    if trimmed.contains("-->") {
        return Line::CommentClose;
    }
    if trimmed.is_empty() || trimmed.starts_with('<') {
        return Line::Blank;
    }
    if trimmed.starts_with('|') {
        let separator = trimmed.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '));
        let cells = if separator {
            vec![]
        } else {
            trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .collect()
        };
        return Line::Table(cells);
    }
    if let Some(rest) = heading_text(trimmed) {
        return Line::Heading(rest);
    }
    match strip_list_marker(trimmed) {
        (true, body) => Line::Item(body),
        (false, body) => Line::Prose(body),
    }
}

#[derive(Default)]
struct State {
    paras: Vec<Para>,
    headings: Vec<usize>,
    current: Option<Para>,
    in_fence: bool,
    in_comment: bool,
}

impl State {
    fn flush(mut self) -> Self {
        if let Some(p) = self.current.take() {
            self.paras.push(p);
        }
        self
    }

    fn step(self, line_no: usize, raw: &str) -> Self {
        let trimmed = raw.trim();
        if self.in_comment {
            let closes = matches!(classify(trimmed), Line::CommentClose);
            return State {
                in_comment: !closes,
                ..self
            };
        }
        if self.in_fence {
            let toggles = matches!(classify(trimmed), Line::Fence);
            return State {
                in_fence: !toggles,
                ..self
            };
        }
        match classify(trimmed) {
            Line::Fence => State {
                in_fence: true,
                ..self.flush()
            },
            Line::CommentOpen => State {
                in_comment: true,
                ..self.flush()
            },
            Line::CommentClose | Line::Blank => self.flush(),
            Line::Table(cells) => {
                let mut s = self.flush();
                s.paras
                    .extend(cells.into_iter().map(|c| Para::cell(line_no, c)));
                s
            }
            Line::Heading(text) => {
                let mut s = self.flush();
                s.headings.push(line_no);
                s.paras.push(Para::heading(line_no, text));
                s
            }
            Line::Item(body) => State {
                current: Some(Para::new(line_no, body)),
                ..self.flush()
            },
            Line::Prose(body) => {
                let mut s = self;
                match s.current.as_mut() {
                    Some(p) => p.append(line_no, body),
                    None => s.current = Some(Para::new(line_no, body)),
                }
                s
            }
        }
    }
}

/// # Panics
/// Panics only if a built-in regex pattern fails to compile, which never happens.
#[must_use]
pub fn parse(text: &str) -> Doc {
    let state = text
        .lines()
        .enumerate()
        .fold(State::default(), |s, (i, raw)| s.step(i + 1, raw))
        .flush();

    let paragraphs: Vec<TextUnit> = state
        .paras
        .iter()
        .map(|p| TextUnit {
            span: 0..p.text.len(),
            text: p.text.clone(),
            line: p.line,
            in_table: p.in_table,
            is_heading: p.is_heading,
        })
        .collect();
    let sentences: Vec<TextUnit> = state
        .paras
        .iter()
        .flat_map(|p| {
            split_sentences(&p.text)
                .into_iter()
                .map(move |(offset, text)| TextUnit {
                    line: p.line_at(offset),
                    span: offset..offset + text.len(),
                    text,
                    in_table: p.in_table,
                    is_heading: p.is_heading,
                })
        })
        .collect();
    let word_count = sentences.iter().map(|s| s.words().len()).sum();
    let whole = TextUnit {
        text: text.to_string(),
        span: 0..text.len(),
        line: 1,
        in_table: false,
        is_heading: false,
    };
    Doc {
        sentences,
        paragraphs,
        whole,
        headings: state.headings,
        word_count,
    }
}

fn heading_text(line: &str) -> Option<&str> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    line.get(hashes..)
        .filter(|rest| rest.starts_with(' '))
        .map(str::trim)
}

fn strip_list_marker(line: &str) -> (bool, &str) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(?:[-*+] (?:\[[ x]\] )?|\d+[.)] |> )").expect("list marker pattern compiles")
    });
    re.find(line)
        .map_or((false, line), |m| (true, line.get(m.end()..).unwrap_or("")))
}

const ABBREVIATIONS: &[&str] = &["e.g.", "i.e.", "vs.", "etc.", "cf.", "no.", "fig."];

/// Full stops inside inline code are masked so they never end a sentence.
fn mask_code(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"`[^`]*`").expect("code span pattern compiles"));
    re.replace_all(text, |m: &regex::Captures| {
        m.get(0)
            .map_or(String::new(), |g| g.as_str().replace(['.', '!', '?'], "x"))
    })
    .into_owned()
}

/// Split on `.`, `!` or `?` followed by whitespace and a capital, digit, quote or markup.
/// Each sentence carries the byte offset where it starts.
fn split_sentences(text: &str) -> Vec<(usize, String)> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?P<p>[.!?]+["')*]*)(?P<ws>\s+)(?P<next>\S)"#)
            .expect("sentence pattern compiles")
    });
    let masked = mask_code(text);
    let boundaries = re.captures_iter(&masked).filter_map(|c| {
        let p = c.name("p")?;
        let next = c.name("next")?.as_str().chars().next()?;
        let starts_sentence = next.is_uppercase()
            || next.is_ascii_digit()
            || matches!(next, '`' | '*' | '"' | '\'' | '[' | '(' | '_');
        let last_word = masked
            .get(..p.end())?
            .split_whitespace()
            .last()
            .unwrap_or("")
            .to_ascii_lowercase();
        let abbreviation = ABBREVIATIONS.iter().any(|a| last_word.ends_with(a));
        (starts_sentence && !abbreviation).then_some((p.end(), c.name("ws")?.end()))
    });
    let (mut out, start) = boundaries.fold(
        (Vec::new(), 0usize),
        |(mut out, start), (end, next_start)| {
            if let Some(s) = text
                .get(start..end)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                out.push((start, s.to_string()));
            }
            (out, next_start)
        },
    );
    if let Some(tail) = text.get(start..).map(str::trim).filter(|s| !s.is_empty()) {
        out.push((start, tail.to_string()));
    }
    out
}

/// Replace inline code with the word `code`, links with their text, and drop emphasis markers.
///
/// # Panics
/// Panics only if a built-in regex pattern fails to compile, which never happens.
#[must_use]
pub fn reduce_inline(s: &str) -> String {
    static CODE: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static STARS: OnceLock<Regex> = OnceLock::new();
    static UNDERSCORE: OnceLock<Regex> = OnceLock::new();
    let code = CODE.get_or_init(|| Regex::new(r"`[^`]*`").expect("code pattern compiles"));
    let link =
        LINK.get_or_init(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").expect("link pattern compiles"));
    let stars = STARS.get_or_init(|| Regex::new(r"\*+").expect("star pattern compiles"));
    let underscore = UNDERSCORE.get_or_init(|| {
        Regex::new(r"(^|[^\w])_+|_+([^\w]|$)").expect("underscore pattern compiles")
    });
    let step1 = code.replace_all(s, "code");
    let step2 = link.replace_all(&step1, "$1");
    let step3 = stars.replace_all(&step2, "");
    underscore.replace_all(&step3, "$1$2").into_owned()
}
