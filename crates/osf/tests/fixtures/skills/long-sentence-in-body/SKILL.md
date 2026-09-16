---
name: long-sentence-in-body
description: >
  Use this skill when the user wants a line-number check that spans a
  folded description across more than one line in the frontmatter.
---

## Overview

Run the check below.

This one very long sentence intentionally uses far more than twenty five words in a single unbroken run so that the long sentence rule in the writing lint fires exactly once on this exact line for the test to check against.

1. Run the check.
2. Report the result.

Stop when the check has run once.

<!-- osf-expect
long-sentence
-->
