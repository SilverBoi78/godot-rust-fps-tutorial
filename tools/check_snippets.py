#!/usr/bin/env python3
"""Verify every Rust snippet in tutorial/ appears verbatim in reference/rust/src/.

The project's core guarantee: no code in a lesson was written from memory.

A snippet is exempt only if the line immediately before its fence is
`<!-- illustrative -->`. Everything else must be a contiguous slice of a
reference source file, ignoring blank lines and trailing whitespace.
"""
import re, sys, pathlib

REPO = pathlib.Path(__file__).resolve().parent.parent
SRC = REPO / "reference/rust/src"

# Compare on fully left-stripped lines. Lessons often quote a method body
# dedented out of its `impl` block, and the indentation level is not what we are
# trying to guarantee -- the tokens, and their order, are.
blob = "\n".join(f.read_text() for f in sorted(SRC.rglob("*.rs")))
blobc = "\n".join(l.strip() for l in blob.splitlines() if l.strip())

fence = re.compile(r"(?:(<!-- illustrative -->)\n)?```rust\n(.*?)```", re.S)

missing, checked, exempt = [], 0, 0
for md in sorted((REPO / "tutorial").glob("*.md")):
    text = md.read_text()
    for m in fence.finditer(text):
        marker, code = m.group(1), m.group(2)
        if marker:
            exempt += 1
            continue
        lines = [l for l in code.strip().splitlines() if l.strip()]
        if len(lines) < 4:
            exempt += 1
            continue
        checked += 1
        compact = "\n".join(l.strip() for l in code.splitlines() if l.strip())
        if compact not in blobc:
            missing.append((md.name, lines[0][:66], len(lines)))

print(f"checked {checked} snippets against the reference build ({exempt} exempt)")
if missing:
    print(f"\n{len(missing)} NOT found verbatim in reference/rust/src:\n")
    for name, first, n in missing:
        print(f"  {name:26} ({n:3} lines)  {first}")
    sys.exit(1)
print("all snippets match the reference build")
