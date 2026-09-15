//! Metadata for every scan rule: why we believe it, and its doc text for
//! `osf explain`. Every rule here uses [`Group::Comprehension`], which
//! [`osf_lint_core::resolve`] always turns into an error, in every context:
//! text that must never reach a public repository is never advisory.

use crate::lint::RuleMeta;
use osf_lint_core::{Class, Group};

pub const SCAN_RULE_META: &[RuleMeta] = &[
    RuleMeta {
        id: "scan-session-link",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a URL that contains `claude.ai/code/session_`.\n\
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
              Bad: See https://claude.ai/code/session_01AbCdEf for the discussion.\n\
              Good: no session link in the text at all.",
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
              A co-author trailer from a coding agent names an internal tool and a session \
              in a commit that becomes part of the project's public history forever.\n\
              ### Class\n\
              house: a policy choice about attribution, not an externally required rule.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: Co-Authored-By: Claude <noreply@example.com>\n\
              Good: no such trailer in the commit.",
        exception: None,
    },
    RuleMeta {
        id: "scan-local-path",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a Windows user path, a drive letter followed by `:\\Users\\`, or a home \
              path, `/home/<name>/` or `/Users/<name>/`.\n\
              ### Why it is bad\n\
              A local path names a real machine and a real account. A public reader gains \
              nothing from it, and it can name the very person who wrote the text.\n\
              ### Class\n\
              correctness: the path is meaningless outside the machine that produced it; no \
              opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad: The file lives at C:\\Users\\pat\\work\\notes.md.\n\
              Good: The file lives at notes.md, relative to the repository root.",
        exception: None,
    },
    RuleMeta {
        id: "scan-foreign-reference",
        class: Class::Correctness,
        group: Group::Comprehension,
        citation: "house",
        doc: "### What it does\n\
              Flags a cross-repository reference, `owner/repo#123`, whose owner is not the \
              configured project owner. Off by default: with no owner configured, the tool \
              cannot tell a foreign reference from the project's own, so it never fires.\n\
              ### Why it is bad\n\
              A reference to another owner's issue tracker, left in by habit or by a copied \
              example, can point a public reader at a private repository they cannot open.\n\
              ### Class\n\
              correctness: once an owner is configured, a reference to a different owner is \
              objectively foreign; no opinion is involved in flagging it.\n\
              ### Citation\n\
              house\n\
              ### Example\n\
              Bad, with project owner `acme`: see other-org/internal-tools#42 for the fix.\n\
              Good: see acme/public-repo#42 for the fix.",
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
              Good: no configured pattern matches anywhere in the text.",
        exception: None,
    },
];

#[must_use]
pub fn rule_meta(id: &str) -> Option<&'static RuleMeta> {
    SCAN_RULE_META.iter().find(|r| r.id == id)
}
