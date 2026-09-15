//! Split Markdown into sentences, paragraphs and a whole-document unit, each
//! with a line number and a byte span. `pulldown-cmark` finds the block and
//! inline structure; a rule or an analyser reads whichever grain its scope
//! asks for, and the shape is the same for all.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
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

/// One block of prose: a paragraph, a heading, or a table cell, built from
/// the raw source it spans. A sentence taken from partway through it can
/// still recover its own line number and byte offset.
struct Block {
    line: usize,
    start_offset: usize,
    text: String,
    /// (offset in `text`, source line, source offset) recorded at each line
    /// break the block joins, so a later offset in `text` can be mapped back.
    breaks: Vec<(usize, usize, usize)>,
    in_table: bool,
    is_heading: bool,
}

impl Block {
    fn new(line: usize, start_offset: usize, in_table: bool, is_heading: bool) -> Self {
        Block {
            line,
            start_offset,
            text: String::new(),
            breaks: Vec::new(),
            in_table,
            is_heading,
        }
    }

    fn push_raw(&mut self, s: &str) {
        self.text.push_str(s);
    }

    /// A line break joins two source lines with one space, as prose does.
    fn push_break(&mut self, source_line: usize, source_offset: usize) {
        self.text.push(' ');
        self.breaks
            .push((self.text.len(), source_line, source_offset));
    }

    fn before(&self, local_offset: usize) -> (usize, usize, usize) {
        self.breaks
            .iter()
            .take_while(|(o, _, _)| *o <= local_offset)
            .last()
            .copied()
            .unwrap_or((0, self.line, self.start_offset))
    }

    fn line_at(&self, local_offset: usize) -> usize {
        self.before(local_offset).1
    }

    fn absolute_at(&self, local_offset: usize) -> usize {
        let (base_local, _, base_absolute) = self.before(local_offset);
        base_absolute + (local_offset - base_local)
    }
}

/// The parse fold's state: the block being built, every block finished so
/// far, and whether we are inside a region that changes how text is read.
#[derive(Default)]
struct Walk {
    blocks: Vec<Block>,
    headings: Vec<usize>,
    current: Option<Block>,
    /// Depth inside a fenced or indented code block, or an HTML block: its
    /// text is never prose.
    skip: u32,
    /// Depth inside emphasis, a strong span, a strikethrough, a link or an
    /// image: already captured whole, so its children are not read again.
    inline_skip: u32,
    in_heading: bool,
    in_cell: bool,
}

impl Walk {
    fn flush(mut self) -> Self {
        if let Some(block) = self.current.take() {
            if block.is_heading {
                self.headings.push(block.line);
            }
            self.blocks.push(block);
        }
        self
    }

    fn open(&mut self, doc_lines: &[usize], offset: usize) -> &mut Block {
        let in_table = self.in_cell;
        let is_heading = self.in_heading;
        self.current.get_or_insert_with(|| {
            Block::new(line_at(doc_lines, offset), offset, in_table, is_heading)
        })
    }
}

/// Emphasis, a strong span, a strikethrough, a link, and an image: `pulldown-cmark`
/// reports the whole span's byte range on the opening event, so it is read
/// once there and its children are skipped.
fn opens_captured_span(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn closes_captured_span(tag: TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link | TagEnd::Image
    )
}

fn raw<'a>(source: &'a str, range: &Range<usize>) -> &'a str {
    source.get(range.clone()).unwrap_or_default()
}

/// An HTML comment, inline or as its own block: never prose, so a
/// suppression marker placed after content on the same line does not lint
/// its own `-->` and ` -- ` as an arrow or an em dash.
fn is_comment(html: &str) -> bool {
    html.trim_start().starts_with("<!--")
}

fn step(
    mut walk: Walk,
    source: &str,
    doc_lines: &[usize],
    event: Event,
    range: Range<usize>,
) -> Walk {
    if walk.skip > 0 {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_)) => {
                walk.skip += 1;
            }
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::MetadataBlock(_)) => {
                walk.skip -= 1;
            }
            _ => {}
        }
        return walk;
    }
    if walk.inline_skip > 0 {
        match event {
            Event::Start(tag) if opens_captured_span(&tag) => walk.inline_skip += 1,
            Event::End(tag) if closes_captured_span(tag) => walk.inline_skip -= 1,
            _ => {}
        }
        return walk;
    }
    match event {
        Event::Start(tag) if opens_captured_span(&tag) => {
            walk.open(doc_lines, range.start)
                .push_raw(raw(source, &range));
            walk.inline_skip += 1;
            walk
        }
        Event::InlineHtml(_) if is_comment(raw(source, &range)) => walk,
        Event::Text(_)
        | Event::Code(_)
        | Event::InlineHtml(_)
        | Event::FootnoteReference(_)
        | Event::TaskListMarker(_) => {
            walk.open(doc_lines, range.start)
                .push_raw(raw(source, &range));
            walk
        }
        Event::SoftBreak | Event::HardBreak => {
            let source_line = line_at(doc_lines, range.end);
            walk.open(doc_lines, range.start)
                .push_break(source_line, range.end);
            walk
        }
        Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_)) => {
            walk = walk.flush();
            walk.skip += 1;
            walk
        }
        Event::Start(Tag::Heading { .. }) => {
            walk = walk.flush();
            walk.in_heading = true;
            walk
        }
        Event::End(TagEnd::Heading(_)) => {
            walk = walk.flush();
            walk.in_heading = false;
            walk
        }
        Event::Start(Tag::TableCell) => {
            walk = walk.flush();
            walk.in_cell = true;
            walk
        }
        Event::End(TagEnd::TableCell) => {
            walk = walk.flush();
            walk.in_cell = false;
            walk
        }
        _ => walk.flush(),
    }
}

/// Byte offset of the start of every line in `text`, so a caller elsewhere
/// in the crate (the suppression scanner) can map an offset to a line too.
pub(crate) fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// 1-based source line containing byte offset `offset`.
pub(crate) fn line_at(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|&s| s <= offset)
}

fn paragraph_unit(block: &Block) -> TextUnit {
    TextUnit {
        span: block.start_offset..block.absolute_at(block.text.len()),
        text: block.text.clone(),
        line: block.line,
        in_table: block.in_table,
        is_heading: block.is_heading,
    }
}

fn sentence_units(block: &Block) -> Vec<TextUnit> {
    split_sentences(&block.text)
        .into_iter()
        .map(|(offset, text)| TextUnit {
            span: block.absolute_at(offset)..block.absolute_at(offset) + text.len(),
            line: block.line_at(offset),
            text,
            in_table: block.in_table,
            is_heading: block.is_heading,
        })
        .collect()
}

/// # Panics
/// Panics only if a built-in regex pattern fails to compile, which never happens.
#[must_use]
pub fn parse(text: &str) -> Doc {
    let doc_lines = line_starts(text);
    let options = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let walk = Parser::new_ext(text, options)
        .into_offset_iter()
        .fold(Walk::default(), |w, (event, range)| {
            step(w, text, &doc_lines, event, range)
        })
        .flush();

    let paragraphs: Vec<TextUnit> = walk.blocks.iter().map(paragraph_unit).collect();
    let sentences: Vec<TextUnit> = walk.blocks.iter().flat_map(sentence_units).collect();
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
        headings: walk.headings,
        word_count,
    }
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
