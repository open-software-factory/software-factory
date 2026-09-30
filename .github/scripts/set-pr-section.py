#!/usr/bin/env python3
"""Replace one marked section of a pull request description in place."""
import re
import subprocess
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: set-pr-section.py <pr-number> <marker> <content-file>", file=sys.stderr)
        return 2
    pr, marker, content_path = sys.argv[1], sys.argv[2], sys.argv[3]
    content = Path(content_path).read_text().strip()
    start, end = f"<!-- osf:{marker}:start -->", f"<!-- osf:{marker}:end -->"
    section = f"{start}\n{content}\n{end}"
    body = subprocess.run(
        ["gh", "pr", "view", pr, "--json", "body", "--jq", ".body"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    pattern = re.compile(re.escape(start) + r".*?" + re.escape(end), re.DOTALL)
    if pattern.search(body):
        new_body = pattern.sub(section, body)
    else:
        separator = "\n\n" if body.strip() else ""
        new_body = body.rstrip() + separator + section + "\n"
    out_path = Path("pr-section-body.md")
    out_path.write_text(new_body)
    subprocess.run(["gh", "pr", "edit", pr, "--body-file", str(out_path)], check=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
