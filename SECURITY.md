# Security

## Reporting a problem

Use the private reporting form on this repository's security page. It opens a
private thread with the maintainers. Only the maintainers can read it.

Do not open a public issue for a security problem.

There is no published contact address. The private thread keeps the report,
the discussion, and the fix in one place.

## What we will do

We will confirm we have read your report. We will tell you whether we agree it
is a problem, and what we plan to do. This project is small, and we cannot
promise a fixed response time.

## What is in scope

The Software Factory is an operating environment for shipping software with
coding agents. It coordinates work across agents, work trackers, repositories,
verification tools, runners, and delivery systems, and it gives an operator
visibility and control over all of that. Every part of it is in scope:

- **The engine and its workflows.** A workflow that can be made to skip a
  gate, run a step it should refuse, or report a result that did not happen.
- **The verifiers and scanners.** A check that can be made to pass on content
  it should refuse. A crafted file that makes the tool run a command, read a
  file outside the directory it was given, or write outside it. A finding that
  prints something it was meant to keep private: the name denylist is the
  clearest case, since a finding must report only a file and a line, and must never
  print the matched text.
- **The gates that decide.** A suppression comment that hides a finding from
  the checks on a pull request, which are meant to ignore suppressions. Any
  way for the author of a change to alter the checks that judge it.
- **The operator console.** Anything that lets a viewer see or do more than
  their role allows, or act on the factory without the audit trail recording
  it.
- **The agent integrations.** A hook, plugin, or adapter that can be driven
  by a crafted reply or file to run code, exfiltrate a session, or bypass the
  stop check.
- **Credentials and identity.** A token, key, or app credential that reaches
  a place an agent or a viewer can read.
- **The release artefacts.** A binary or package that does not match its
  source, or that can be substituted on the way to a user.
- **Dependencies.** A known problem in something this project depends on that
  we have not picked up.

## What is out of scope

A rule that reports something wrong, or misses something, is a bug rather than
a security problem. Please open an ordinary issue for it.

The checks that run before a commit raise the bar. They do not seal it. Anyone
can pass a flag to skip a local hook, and that is a property of the version
control system rather than a flaw here. The checks that run on a pull request
are the ones that decide, and they run where a contributor cannot change them.
