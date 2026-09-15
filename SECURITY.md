# Security

## Reporting a problem

Use the private reporting form on this repository's security page. It opens a
private thread with the maintainers. Only the maintainers can read it.

Do not open a public issue for a security problem.

There is no published contact address on purpose. A private thread keeps the
report, the discussion, and the fix in one place, and it needs no shared inbox.

## What we will do

We will confirm we have read your report. We will tell you whether we agree it
is a problem, and what we plan to do.

This project is small. We cannot promise a fixed response time, and we would
rather say so than publish a deadline we might miss.

## What is in scope

This project runs checks over files and over the output of coding agents. The
problems worth reporting are the ones where that goes wrong:

- A check can be made to pass on content it should refuse.
- A crafted file causes the tool to run a command, read a file outside the
  directory it was given, or write outside it.
- The tool prints something it was meant to keep private. The name denylist is
  the clearest case. A finding must report a file and a line number, never the
  text that matched.
- A suppression comment hides a finding from the checks that run on a pull
  request, which are meant to ignore suppressions.
- A dependency of this project has a known problem we have not picked up.

## What is out of scope

A rule that reports something wrong, or misses something, is a bug rather than
a security problem. Please open an ordinary issue for it.

The checks that run before a commit raise the bar. They do not seal it. Anyone
can pass a flag to skip a local hook, and that is a property of the version
control system rather than a flaw here. The checks that run on a pull request
are the ones that decide, and they run where a contributor cannot change them.
