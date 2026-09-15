//! Lint for a skill folder's `SKILL.md`: the frontmatter, whether the body
//! reads like steps for an agent or like a manual for a person, script
//! version pinning, and context-injection safety.
//!
//! `agnix`, a separate linter, already checks the skill name format, the
//! description and compatibility length, required and unknown frontmatter
//! keys, body length, link resolution, absolute paths, and reference depth.
//! This lint does not repeat those checks; [`UNCHECKED_NOTE`] says so on
//! every run, so a clean result here is never read as a full validation.

use super::meta::RuleMeta;
use super::{lint_writing, KnownNames};
use crate::config::{SkillConfig, WritingConfig};
use osf_lint_core::{resolve, Class, Context, Finding, Group, Level};
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

/// The checks `osf lint skill` deliberately does not perform, and the tool
/// that does them. Printed on every run of the command.
pub const UNCHECKED_NOTE: &str = "osf does not check the skill name format, the description or \
compatibility length, required or unknown frontmatter keys, body length, link resolution, \
absolute paths, or reference depth. Run agnix for those checks.";

/// A finding tied to the file it came from, since a skill folder holds more than one file.
pub struct SkillFinding {
    pub file: String,
    pub finding: Finding,
}

/// Common imperative verbs a task-shaped skill opens its steps with.
const IMPERATIVE_VERBS: &[&str] = &[
    "Run", "Read", "Write", "Check", "Open", "Create", "Add", "Remove", "Delete", "Use", "Call",
    "Post", "Put", "Set", "Get", "Fetch", "Install", "Build", "Test", "Verify", "Stop", "Start",
    "Ask", "Report", "Record", "Fill", "Copy", "Move", "Rename", "Edit", "Apply", "Render",
    "Print", "Return", "Skip", "Repeat", "Compare", "Confirm", "Choose", "Pick", "Select",
    "Search", "Find", "List", "Show", "Save", "Load", "Parse", "Send", "Wait", "Close", "Merge",
    "Push", "Pull", "Commit", "Branch", "Tag", "Release", "Deploy", "Follow", "Do", "Make", "Keep",
    "Leave", "Name", "Say", "Tell", "Give", "Take", "Split", "Join", "Sort", "Count", "Log",
];

/// Phrases that tell an agent when to stop a run of numbered steps.
const DONE_PHRASES: &[&str] = &[
    "done when",
    "stop when",
    "finish when",
    "complete when",
    "until",
    "only proceed",
    "hand off when",
    "exit when",
];

/// Phrases the description must not use; a skill is instructions, not a first-person pitch.
const FIRST_PERSON_PHRASES: &[&str] = &["I can", "I will", "I help"];

struct FmEntry {
    key: String,
    value: String,
    line: usize,
}

struct Frontmatter {
    entries: Vec<FmEntry>,
    body_start: usize,
}

/// Reads `<dir>/SKILL.md` and runs every kept skill rule over it, the
/// writing lint over its body, and the script-pin check over every script
/// under `<dir>/scripts`.
///
/// # Errors
///
/// Returns `Err` when `SKILL.md` cannot be read.
pub fn lint_skill(
    dir: &Path,
    cfg: &SkillConfig,
    known: &KnownNames,
    writing: &WritingConfig,
) -> Result<Vec<SkillFinding>, String> {
    let path = dir.join("SKILL.md");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let lines: Vec<&str> = text.lines().collect();

    let fm = parse_frontmatter(&lines);
    let mut findings = duplicate_findings(&fm.entries);
    let (desc, desc_line) = description(&fm.entries);
    findings.extend(trigger_findings(&desc, desc_line, cfg));
    findings.extend(first_person_findings(&desc, desc_line));
    findings.extend(manual_shape_findings(&lines, fm.body_start, cfg));
    findings.extend(context_injection_findings(&lines, fm.body_start));

    let mut out: Vec<SkillFinding> = findings
        .into_iter()
        .map(|finding| SkillFinding {
            file: "SKILL.md".to_string(),
            finding,
        })
        .collect();
    out.extend(body_writing_findings(&lines, fm.body_start, known, writing));
    out.extend(script_findings(dir));
    resolve_and_explain(&mut out);
    out.sort_by(|a, b| {
        (a.file.as_str(), a.finding.line, a.finding.rule).cmp(&(
            b.file.as_str(),
            b.finding.line,
            b.finding.rule,
        ))
    });
    Ok(out)
}

/// Sets each finding's level and remediation from its rule's class and
/// group, resolved against the skill context, and points the message at
/// `osf explain <rule-id>`.
fn resolve_and_explain(findings: &mut [SkillFinding]) {
    for sf in findings.iter_mut() {
        let Some(meta) = rule_meta(sf.finding.rule) else {
            continue;
        };
        let (level, remediation) = resolve(meta.class, meta.group, Context::Skill, meta.exception);
        sf.finding.level = level;
        sf.finding.remediation = remediation;
        sf.finding.message = format!(
            "{} (see `osf explain {}`)",
            sf.finding.message, sf.finding.rule
        );
    }
}

/// Runs the writing lint over the body only, under `Context::Skill`, and
/// offsets every line by `body_start` so it points at the real line in
/// `SKILL.md` rather than a line relative to the body.
fn body_writing_findings(
    lines: &[&str],
    body_start: usize,
    known: &KnownNames,
    writing: &WritingConfig,
) -> Vec<SkillFinding> {
    let body = lines.get(body_start..).unwrap_or(&[]).join("\n");
    lint_writing(&body, known, writing, Context::Skill, false, false)
        .into_iter()
        .map(|mut finding| {
            finding.line += body_start;
            SkillFinding {
                file: "SKILL.md".to_string(),
                finding,
            }
        })
        .collect()
}

fn re(cell: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("rule pattern compiles"))
}

fn finding_at(line: usize, rule: &'static str, message: String, excerpt: &str) -> Finding {
    Finding::new(rule, Level::Error, line, message, excerpt.to_string())
}

fn parse_frontmatter(lines: &[&str]) -> Frontmatter {
    static KEY: OnceLock<Regex> = OnceLock::new();
    let key_re = re(&KEY, r"^([A-Za-z][A-Za-z0-9_-]*):\s?(.*)$");
    let opens = lines.first().is_some_and(|l| l.trim() == "---");
    if !opens {
        return Frontmatter {
            entries: Vec::new(),
            body_start: 0,
        };
    }
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, l)| l.trim() == "---")
        .map(|(i, _)| i);
    let Some(close) = close else {
        return Frontmatter {
            entries: Vec::new(),
            body_start: lines.len(),
        };
    };
    // An indented line continues the entry above it: a `>` or `|` block scalar.
    let entries = lines.get(1..close).unwrap_or(&[]).iter().enumerate().fold(
        Vec::<FmEntry>::new(),
        |mut acc, (i, raw)| {
            if raw.starts_with(char::is_whitespace) {
                if let Some(last) = acc.last_mut() {
                    if !last.value.is_empty() {
                        last.value.push(' ');
                    }
                    last.value.push_str(raw.trim());
                }
                return acc;
            }
            if let Some(caps) = key_re.captures(raw) {
                let key = caps.get(1).map(|g| g.as_str().to_string());
                let value = caps.get(2).map(|g| g.as_str().trim());
                if let (Some(key), Some(value)) = (key, value) {
                    let value = if matches!(value, ">" | "|" | ">-" | "|-") {
                        String::new()
                    } else {
                        value.trim_matches('"').to_string()
                    };
                    acc.push(FmEntry {
                        key,
                        value,
                        line: i + 2,
                    });
                }
            }
            acc
        },
    );
    Frontmatter {
        entries,
        body_start: close + 1,
    }
}

fn duplicate_findings(entries: &[FmEntry]) -> Vec<Finding> {
    let mut seen: HashSet<&str> = HashSet::new();
    entries
        .iter()
        .filter(|e| !seen.insert(e.key.as_str()))
        .map(|e| {
            finding_at(
                e.line,
                "skill-frontmatter-duplicate",
                format!("the key \"{}\" appears twice in the frontmatter", e.key),
                &e.key,
            )
        })
        .collect()
}

fn description(entries: &[FmEntry]) -> (String, usize) {
    entries
        .iter()
        .find(|e| e.key == "description")
        .map_or((String::new(), 1), |e| (e.value.clone(), e.line))
}

fn trigger_findings(desc: &str, line: usize, cfg: &SkillConfig) -> Vec<Finding> {
    if desc.is_empty() {
        return vec![];
    }
    let lower = desc.to_lowercase();
    if cfg
        .trigger_phrases
        .iter()
        .any(|p| lower.contains(p.as_str()))
    {
        return vec![];
    }
    vec![finding_at(
        line,
        "skill-description-no-trigger",
        "say when an agent should use this skill".to_string(),
        desc,
    )]
}

fn first_person_findings(desc: &str, line: usize) -> Vec<Finding> {
    FIRST_PERSON_PHRASES
        .iter()
        .filter(|p| desc.contains(*p))
        .map(|p| {
            finding_at(
                line,
                "skill-first-person",
                "write the description in the third person".to_string(),
                p,
            )
        })
        .collect()
}

fn is_numbered_list_line(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"^\s*\d+[.)]\s").is_match(line)
}

fn is_imperative_line(line: &str) -> bool {
    line.split_whitespace().next().is_some_and(|w| {
        IMPERATIVE_VERBS.contains(&w.trim_end_matches(|c: char| !c.is_alphanumeric()))
    })
}

fn is_descriptive_line(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(
        &RE,
        r"^\s*[A-Z][a-z]+ (is|are|was|were|has|have|consists|works|contains|describes|represents) ",
    )
    .is_match(line)
}

fn is_code_fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

fn is_heading_line(line: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"^#{2,3}\s+").is_match(line)
}

/// The first section of the body: prose before the first heading, when any
/// non-blank line comes before it; otherwise the content under that heading,
/// up to the next one. The whole body, when it carries no heading at all.
fn first_section<'a>(body: &'a [&'a str]) -> &'a [&'a str] {
    let headings: Vec<usize> = body
        .iter()
        .enumerate()
        .filter(|(_, l)| is_heading_line(l))
        .map(|(i, _)| i)
        .collect();
    let Some(&first) = headings.first() else {
        return body;
    };
    let before = body.get(..first).unwrap_or(&[]);
    if before.iter().any(|l| !l.trim().is_empty()) {
        return before;
    }
    let end = headings.get(1).copied().unwrap_or(body.len());
    body.get(first + 1..end).unwrap_or(&[])
}

fn is_step_shaped(section: &[&str]) -> bool {
    section.iter().any(|l| is_numbered_list_line(l))
        || section.iter().any(|l| is_code_fence(l))
        || section.iter().any(|l| is_imperative_line(l))
}

fn count_paragraphs(section: &[&str]) -> usize {
    section
        .iter()
        .fold((0usize, false), |(count, in_para), line| {
            if line.trim().is_empty() {
                (count, false)
            } else if in_para {
                (count, true)
            } else {
                (count + 1, true)
            }
        })
        .0
}

fn count_words(section: &[&str]) -> usize {
    section.iter().map(|l| l.split_whitespace().count()).sum()
}

/// `true`, plus the finding, when the first section is not step-shaped and
/// runs past the configured paragraph or word budget.
fn overview_findings(lines: &[&str], body_start: usize, cfg: &SkillConfig) -> (bool, Vec<Finding>) {
    let body = lines.get(body_start..).unwrap_or(&[]);
    let section = first_section(body);
    if is_step_shaped(section) {
        return (false, vec![]);
    }
    let paragraphs = count_paragraphs(section);
    let words = count_words(section);
    if paragraphs <= cfg.overview_max_paragraphs && words <= cfg.overview_max_words {
        return (false, vec![]);
    }
    let finding = finding_at(
        body_start + 1,
        "skill-first-section-is-overview",
        format!(
            "the opening is {paragraphs} paragraph(s) and {words} word(s); past {} \
             paragraph(s) or {} word(s), put the step first",
            cfg.overview_max_paragraphs, cfg.overview_max_words
        ),
        "first section",
    );
    (true, vec![finding])
}

fn descriptive_over_imperative(body: &[&str]) -> bool {
    let imperative = body
        .iter()
        .filter(|l| is_numbered_list_line(l) || is_imperative_line(l))
        .count();
    let descriptive = body.iter().filter(|l| is_descriptive_line(l)).count();
    descriptive > imperative
}

fn no_done_condition(body: &[&str], min_steps: usize) -> bool {
    let steps = body.iter().filter(|l| is_numbered_list_line(l)).count();
    if steps <= min_steps {
        return false;
    }
    let joined = body.join("\n").to_lowercase();
    !DONE_PHRASES.iter().any(|p| joined.contains(p))
}

fn manual_shape_findings(lines: &[&str], body_start: usize, cfg: &SkillConfig) -> Vec<Finding> {
    let body = lines.get(body_start..).unwrap_or(&[]);
    let (overview, mut out) = overview_findings(lines, body_start, cfg);
    let descriptive = descriptive_over_imperative(body);
    let no_done = no_done_condition(body, cfg.manual_min_steps);
    if descriptive {
        out.push(finding_at(
            body_start + 1,
            "skill-descriptive-over-imperative",
            "more lines describe the skill than tell an agent what to do".to_string(),
            "descriptive lines",
        ));
    }
    if no_done {
        out.push(finding_at(
            body_start + 1,
            "skill-no-done-condition",
            "the steps never say when to stop".to_string(),
            "no done condition",
        ));
    }
    let fired: Vec<&str> = [
        (overview, "skill-first-section-is-overview"),
        (descriptive, "skill-descriptive-over-imperative"),
        (no_done, "skill-no-done-condition"),
    ]
    .into_iter()
    .filter_map(|(b, name)| b.then_some(name))
    .collect();
    if fired.len() >= 2 {
        out.push(finding_at(
            body_start + 1,
            "skill-reads-as-manual",
            format!("this skill reads like a manual: {}", fired.join(", ")),
            "reads as manual",
        ));
    }
    out
}

fn context_injection_findings(lines: &[&str], body_start: usize) -> Vec<Finding> {
    static INERT: OnceLock<Regex> = OnceLock::new();
    static CMD: OnceLock<Regex> = OnceLock::new();
    let inert_re = re(&INERT, "`!`");
    let cmd_re = re(&CMD, "!`([^`]*)`");
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate().skip(body_start) {
        let line_no = i + 1;
        if inert_re.is_match(raw) {
            out.push(finding_at(
                line_no,
                "skill-context-injection",
                "do not show the context-injection syntax, even escaped".to_string(),
                raw.trim(),
            ));
            continue;
        }
        for cap in cmd_re.captures_iter(raw) {
            if let Some(cmd) = cap.get(1).map(|g| g.as_str()) {
                if cmd.contains("&&") || cmd.contains('|') || cmd.contains("$(") {
                    out.push(finding_at(
                        line_no,
                        "skill-context-injection",
                        "a context-injection command must run one plain command".to_string(),
                        cmd,
                    ));
                }
            }
        }
    }
    out
}

/// Scans every file under `<dir>/scripts` for an install line with no
/// pinned version. Windows has no executable bit, so a script's own
/// permissions are never part of this check.
fn script_findings(dir: &Path) -> Vec<SkillFinding> {
    static LATEST: OnceLock<Regex> = OnceLock::new();
    static NPM: OnceLock<Regex> = OnceLock::new();
    static PIP: OnceLock<Regex> = OnceLock::new();
    let latest_re = re(&LATEST, r":latest\b");
    let npm_re = re(&NPM, r"npm install -g (?:@[^\s/]+/)?[^\s@]+(@\S+)?");
    let pip_re = re(&PIP, r"pip install [^\s=]+(==\S+)?");
    let scripts_dir = dir.join("scripts");
    let Ok(read_dir) = std::fs::read_dir(&scripts_dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for (i, line) in text.lines().enumerate() {
            out.extend(
                unpinned_findings(line, i + 1, latest_re, npm_re, pip_re).map(|finding| {
                    SkillFinding {
                        file: rel.clone(),
                        finding,
                    }
                }),
            );
        }
    }
    out
}

fn unpinned_findings<'a>(
    line: &'a str,
    line_no: usize,
    latest_re: &Regex,
    npm_re: &Regex,
    pip_re: &Regex,
) -> impl Iterator<Item = Finding> + 'a {
    let latest = latest_re.is_match(line).then(|| {
        finding_at(
            line_no,
            "skill-script-unpinned",
            "pin the version instead of :latest".to_string(),
            line.trim(),
        )
    });
    let npm = npm_re
        .captures(line)
        .filter(|c| c.get(1).is_none())
        .map(|_| {
            finding_at(
                line_no,
                "skill-script-unpinned",
                "pin the package version, as in name@1.2.3 or @scope/name@1.2.3".to_string(),
                line.trim(),
            )
        });
    let pip = pip_re
        .captures(line)
        .filter(|c| c.get(1).is_none())
        .map(|_| {
            finding_at(
                line_no,
                "skill-script-unpinned",
                "pin the package version, as in name==1.2.3".to_string(),
                line.trim(),
            )
        });
    latest.into_iter().chain(npm).chain(pip)
}

const SKILL_RULE_META: &[RuleMeta] = &[
    RuleMeta {
        id: "skill-description-no-trigger",
        class: Class::Evidence,
        group: Group::Comprehension,
        citation: "Anthropic's Agent Skills documentation: a model chooses which skill to load \
                   by reading its name and description alone, before any of the skill's own \
                   body loads.",
        doc: "### What it does\n\
              Flags a skill description that carries none of the configured trigger \
              phrases, such as \"use when\", \"when the user\", or \"fires on\".\n\
              ### Why it is bad\n\
              An agent decides which skill to load from the description alone. A \
              description with no stated condition is never matched to the situation it \
              was written for.\n\
              ### Class\n\
              evidence: Anthropic's own Agent Skills documentation describes the \
              selection mechanism this rule protects; no controlled measurement number \
              is cited alongside it.\n\
              ### Citation\n\
              Anthropic's Agent Skills documentation: a model chooses which skill to load \
              by reading its name and description alone, before any of the skill's own \
              body loads.\n\
              ### Example\n\
              Bad: description: Checks a folder for common problems.\n\
              Good: description: Use this skill when the user wants a quick health check \
              of a project folder.",
        exception: None,
    },
    RuleMeta {
        id: "skill-reads-as-manual",
        class: Class::House,
        group: Group::Style,
        citation: "house",
        doc: "### What it does\n\
              Flags a skill body where at least two of three signals fire together: the \
              opening reads as background rather than a step, more lines describe the \
              skill than tell an agent what to do, and a run of numbered steps never says \
              when to stop.\n\
              ### Why it is bad\n\
              A skill file is read by an agent mid-task, not studied like a manual. \
              Background explanation and passive description cost turns an agent should \
              spend acting.\n\
              ### Class\n\
              house: our own taste. It is a composite of three heuristics, each of which \
              can misfire on its own, so watch its false-positive rate.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: a skill that opens with three paragraphs of background, describes the \
              tool in passive sentences, and lists steps with no stopping point.\n\
              Good: a skill that opens with the first step and states when the run is \
              done.",
        exception: None,
    },
    RuleMeta {
        id: "skill-first-section-is-overview",
        class: Class::House,
        group: Group::Style,
        citation: "house",
        doc: "### What it does\n\
              Flags the first section of a skill body when it is not step-shaped (no \
              numbered list, no code fence, no line opening with an imperative verb) and \
              it runs past a paragraph count or a word count. Both limits live in the \
              `[skill]` config section; the compiled defaults are 2 paragraphs and 120 \
              words.\n\
              ### Why it is bad\n\
              A short problem statement before the first step is fine. Past the budget, \
              the opening reads as background an agent must study before it can act, \
              which is what a manual does and a skill should not.\n\
              ### Class\n\
              house: our own taste; no external standard sets these limits.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: four paragraphs of background before the first numbered step.\n\
              Good: one short paragraph of context, then the first step.",
        exception: None,
    },
    RuleMeta {
        id: "skill-descriptive-over-imperative",
        class: Class::House,
        group: Group::Style,
        citation: "house",
        doc: "### What it does\n\
              Flags a skill body where more lines describe the skill in the third person \
              (\"Checker is a folder health tool\") than tell an agent what to do, as a \
              numbered step or a line opening with an imperative verb.\n\
              ### Why it is bad\n\
              A skill is instructions for an agent to follow, not a description of a tool \
              for a person to read about.\n\
              ### Class\n\
              house: our own taste, no external standard requires this shape.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: Checker is a folder health tool for small projects.\n\
              Good: Run the folder health check before every release.",
        exception: None,
    },
    RuleMeta {
        id: "skill-no-done-condition",
        class: Class::House,
        group: Group::Style,
        citation: "house",
        doc: "### What it does\n\
              Flags a run of numbered steps, longer than the configured minimum (3 by \
              default), that never says when to stop: none of \"done when\", \"stop \
              when\", \"finish when\", \"complete when\", \"until\", \"only proceed\", \
              \"hand off when\", or \"exit when\" appears anywhere in the body.\n\
              ### Why it is bad\n\
              An agent running a long list of steps with no stopping condition either \
              stops too early or keeps going past the point the task was done.\n\
              ### Class\n\
              house: our own taste, no external standard requires this shape.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: four numbered steps and no line saying when the run is finished.\n\
              Good: Stop when every check has run once, whether or not it found a \
              problem.",
        exception: None,
    },
    RuleMeta {
        id: "skill-first-person",
        class: Class::House,
        group: Group::Style,
        citation: "house",
        doc: "### What it does\n\
              Flags a description written in the first person: \"I can\", \"I will\", or \
              \"I help\".\n\
              ### Why it is bad\n\
              A skill's description is instructions an agent reads about itself in the \
              third person, not a pitch from the skill to the reader.\n\
              ### Class\n\
              house: our own taste, no external standard requires this shape.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: description: I can check a folder for common problems.\n\
              Good: description: Use this skill when the user wants a quick health check \
              of a project folder.",
        exception: None,
    },
    RuleMeta {
        id: "skill-frontmatter-duplicate",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a frontmatter key that appears more than once, such as two \
              `description:` lines.\n\
              ### Why it is bad\n\
              A duplicate key resolves one way or another with no warning from the \
              parser. The reader cannot tell which value takes effect.\n\
              ### Class\n\
              correctness: the frontmatter is objectively ambiguous; no opinion is \
              involved in checking for it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: description: First line.\\ndescription: Second line.\n\
              Good: one description key, one value.",
        exception: None,
    },
    RuleMeta {
        id: "skill-script-unpinned",
        class: Class::Security,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a line under `scripts/` that installs a dependency with no pinned \
              version: `:latest`, `npm install -g` with no `@version` on the package \
              (a scoped package, such as `@scope/name`, included), or `pip install` with \
              no `==version`.\n\
              ### Why it is bad\n\
              An unpinned install can resolve to a different, unreviewed version between \
              the run that read this skill and the run that executes it. A skill's \
              scripts run with the same trust as the skill itself.\n\
              ### Class\n\
              security: a real risk in a file an agent executes; an unpinned dependency \
              can change what code runs with no change to the skill file.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: npm install -g @scope/tool\n\
              Good: npm install -g @scope/tool@1.4.2",
        exception: None,
    },
    RuleMeta {
        id: "skill-context-injection",
        class: Class::Security,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags the context-injection execution marker written inline, even escaped \
              (a literal `` `!` `` sequence), and a `` !`command` `` block whose command \
              chains more than one instruction with `&&`, `|`, or `$(`.\n\
              ### Why it is bad\n\
              A tool that renders this file for an agent may still execute an escaped \
              marker, and a chained command runs more than the one plain command a \
              reader can audit at a glance. `agnix` checks a different part of the same \
              surface, so this rule complements it rather than repeating it.\n\
              ### Class\n\
              security: a real risk in a file an agent reads and, through a \
              context-injection-aware tool, may execute.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: !`git status && git push`\n\
              Good: !`git status`",
        exception: None,
    },
];

#[must_use]
pub(crate) fn rule_meta(id: &str) -> Option<&'static RuleMeta> {
    SKILL_RULE_META.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE_IDS: &[&str] = &[
        "skill-description-no-trigger",
        "skill-reads-as-manual",
        "skill-first-section-is-overview",
        "skill-descriptive-over-imperative",
        "skill-no-done-condition",
        "skill-first-person",
        "skill-frontmatter-duplicate",
        "skill-script-unpinned",
        "skill-context-injection",
    ];

    #[test]
    fn every_kept_rule_has_metadata() {
        for id in RULE_IDS {
            assert!(rule_meta(id).is_some(), "no metadata for {id}");
        }
        assert_eq!(SKILL_RULE_META.len(), RULE_IDS.len());
    }

    #[test]
    fn a_house_rule_says_so_in_its_citation() {
        for meta in SKILL_RULE_META {
            if meta.class == Class::House {
                assert_eq!(meta.citation, "house", "{}", meta.id);
            }
            assert!(
                !meta.doc.to_lowercase().contains("asd-ste100 says"),
                "{}",
                meta.id
            );
        }
    }
}
