---
name: running-sub-agents
description: Use this skill when you dispatch a sub-agent, or a batch of sub-agents, to do work.
---

1. Run every command a dispatched agent runs in the foreground. A backgrounded long command can stall forever while nothing notices.
2. Report a sub-agent's status from evidence only, such as a pushed branch, an opened pull request, or a changed file's timestamp. The mere absence of a stall notification is never evidence of progress.

Stop when every dispatched command ran in the foreground, and its status report rests on real evidence.
