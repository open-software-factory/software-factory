//! Metadata for every scan rule: why we believe it, and its doc text for
//! `osf explain`. Every rule here uses [`Group::Comprehension`], which
//! [`osf_lint_core::resolve`] always turns into an error, in every context:
//! text that must never reach a public repository is never advisory.
//!
//! Every doc text ends with a Coverage section that names what the rule
//! checks and what it does not. A test keeps those sections in step with
//! [`crate::agents::AGENTS`], so a rule cannot claim an agent it does not
//! know, or drop one without the doc changing.

use crate::lint::RuleMeta;
use osf_lint_core::{Class, Group};

pub const SCAN_RULE_META: &[RuleMeta] = &[
    RuleMeta {
        id: "scan-session-link",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a link to a hosted coding-agent session, for every supported agent \
              that has hosted sessions, plus any prefix added under `[scan] session_links`.\n\
              ### Why it is bad\n\
              A session link points at one private conversation. A public reader cannot \
              open it, and its presence tells them work here runs through a coding agent \
              session that was never meant to be shared.\n\
              ### Class\n\
              correctness: the link is dead weight for a public reader in every case; no \
              opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: a link to an agent's session page, pasted into a discussion.\n\
              Good: no session link in the text at all.\n\
              ### Coverage\n\
              Hosted session links for: claude code, codex, opencode. Agents whose \
              sessions live only on disk leave a path rather than a link: dsh, pi, omp, \
              copilot. Those are covered by `scan-agent-state-path`. A host this list \
              does not name is not checked unless it is added in the configuration.",
        exception: None,
    },
    RuleMeta {
        id: "scan-agent-state-path",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a path into a coding agent's own state directory, where it keeps \
              sessions, transcripts, history, or logs, for every supported agent.\n\
              ### Why it is bad\n\
              Such a path names a private conversation on one machine. A public reader \
              cannot open it, and it discloses which agent produced the work and where \
              that agent keeps its records.\n\
              ### Class\n\
              correctness: the path is meaningless outside the machine that produced it; no \
              opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: a path that runs from an agent's dot directory into its sessions folder \
              and on to a transcript file.\n\
              Good: an agent's committed configuration file, such as its hooks file, which \
              is repository content and is not flagged.\n\
              ### Coverage\n\
              The state directories of: dsh, pi, omp, opencode, codex, claude code, copilot. \
              Only the conversation segments under them are flagged: sessions, projects, \
              transcripts, history, logs. A configuration file under the same directory is \
              not.",
        exception: None,
    },
    RuleMeta {
        id: "scan-coauthor-trailer",
        class: Class::House,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a line that begins `Co-Authored-By:`.\n\
              ### Why it is bad\n\
              A co-author trailer from a coding agent names a tool and a session in a \
              commit that becomes part of the project's public history forever.\n\
              ### Class\n\
              house: a policy choice about attribution, not an externally required rule.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: Co-Authored-By: An Agent <noreply@example.com>\n\
              Good: no such trailer in the commit.\n\
              ### Coverage\n\
              The trailer line itself, whichever agent wrote it. The project's own \
              `Code-Generator:` trailer is the accepted form and is not flagged.",
        exception: None,
    },
    RuleMeta {
        id: "scan-local-path",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a path that names a real machine or a real account on any platform \
              this project runs on: a drive letter and a user folder on Windows, with \
              either slash; a Windows network share; a home directory on Linux or macOS, \
              or the root account's; and a Windows user folder seen through the Windows \
              Subsystem for Linux.\n\
              ### Why it is bad\n\
              A local path names a real machine and a real account. A public reader gains \
              nothing from it, and it can name the very person who wrote the text.\n\
              ### Class\n\
              correctness: the path is meaningless outside the machine that produced it; no \
              opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: a drive letter, the user folder, an account name, and then the file. \
              The shape is not written out here, because this project scans its own \
              source and the rule would flag it.\n\
              Good: The file lives at notes.md, relative to the repository root.\n\
              ### Coverage\n\
              Windows, a Windows network share, Windows Subsystem for Linux, Linux, the \
              Linux root account, macOS. A path written through the home shorthand or an \
              environment variable names no machine and no account, so it is not flagged.",
        exception: None,
    },
    RuleMeta {
        id: "scan-foreign-reference",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a cross-repository reference, `owner/repo#<number>`, whose owner is not \
              this project's. The owner is `[scan] project_owner` when set, else the owner \
              segment of the `origin` remote. With neither, the rule does not run and the \
              scan says so.\n\
              ### Why it is bad\n\
              A reference to another owner's issue tracker, left in by habit or by a copied \
              example, can point a public reader at a private repository they cannot open.\n\
              ### Class\n\
              correctness: once an owner is known, a reference to a different owner is \
              objectively foreign; no opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad, with owner `acme`: see other-org/internal-tools#<number> for the fix.\n\
              Good: see acme/public-repo#<number> for the fix.\n\
              ### Coverage\n\
              The owner only. A reference to another repository under the same owner \
              passes, whatever that repository's visibility, because visibility is not \
              checked. A reference to a public repository under another owner is still \
              flagged, for a person to confirm.",
        exception: None,
    },
    RuleMeta {
        id: "scan-denied-name",
        class: Class::Security,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a line matching a configured denylist pattern. The pattern and the \
              matched text are never printed: only the file and the line number are, since \
              printing either would leak the very thing this rule protects.\n\
              ### Why it is bad\n\
              A denied name is something that must never reach a public repository at all, \
              such as a real person's name or a former employer's name kept out of the \
              project by policy.\n\
              ### Class\n\
              security: a real risk to the people or projects the denylist protects; no \
              opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: a line matching a configured pattern (not shown here, for the same reason \
              a finding never shows it).\n\
              Good: no configured pattern matches anywhere in the text.\n\
              ### Coverage\n\
              Exactly the configured patterns, case-insensitively. With an empty list the \
              rule does not run.",
        exception: None,
    },
];

#[must_use]
pub fn rule_meta(id: &str) -> Option<&'static RuleMeta> {
    SCAN_RULE_META.iter().find(|r| r.id == id)
}
