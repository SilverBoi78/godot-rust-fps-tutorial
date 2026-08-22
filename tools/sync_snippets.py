#!/usr/bin/env python3
"""Re-sync tutorial Rust snippets from the reference build.

For each non-illustrative snippet that no longer matches, find the reference
file containing its first and last lines and replace the snippet with the exact
contiguous slice between them. Run `check_snippets.py` afterwards.
"""
import re, sys, pathlib

REPO = pathlib.Path(__file__).resolve().parent.parent
SRC = REPO / "reference/rust/src"
files = {f: f.read_text().splitlines() for f in sorted(SRC.rglob("*.rs"))}

def compact(lines):
    return [l.rstrip() for l in lines if l.strip()]

def find_slice(block):
    """Find the reference file this snippet came from, then return the exact
    contiguous slice spanning its first and last recognisable lines.

    Matching on the first line alone is too brittle -- rustfmt rewraps `use`
    statements, so a snippet's opening line may no longer exist verbatim. We
    instead pick the file with the most overlapping lines and anchor on the
    first and last snippet lines that file actually contains.
    """
    want = [l for l in compact(block.splitlines()) if len(l) > 12]
    if not want:
        return None

    best, best_score = None, 0
    for path, lines in files.items():
        present = {l.rstrip() for l in lines}
        score = sum(1 for l in want if l in present)
        if score > best_score:
            best, best_score = path, score
    if best is None or best_score < len(want) * 0.8:
        return None

    lines = files[best]
    present = {l.rstrip(): None for l in lines}
    anchors = [l for l in want if l in present]
    if not anchors:
        return None

    first = next(i for i, l in enumerate(lines) if l.rstrip() == anchors[0])
    last = max(i for i, l in enumerate(lines) if l.rstrip() == anchors[-1])

    # Attributes are short, so the anchor filter drops them; walk back over any
    # `#[...]` lines directly above the anchor so `#[godot_api]` is not lost.
    while first > 0 and lines[first - 1].lstrip().startswith("#["):
        first -= 1

    # If the snippet opened with imports, start at the file's first `use` line.
    # rustfmt rewraps long `use` statements, so the snippet's original opening
    # line may not exist verbatim any more and the anchor lands too far in.
    if compact(block.splitlines())[0].startswith("use "):
        uses = [i for i, l in enumerate(lines) if l.startswith("use ")]
        if uses and uses[0] < first:
            first = uses[0]

    return "\n".join(lines[first:last + 1])

fence = re.compile(r"(<!-- illustrative -->\n)?```rust\n(.*?)```", re.S)
changed = 0
for md in sorted((REPO / "tutorial").glob("*.md")):
    text = md.read_text()
    out, last = [], 0
    for m in fence.finditer(text):
        marker, block = m.group(1), m.group(2)
        if marker or len([l for l in block.strip().splitlines() if l.strip()]) < 4:
            continue
        blobc = "\n".join(
            "\n".join(compact(v)) for v in files.values()
        )
        if "\n".join(compact(block.splitlines())) in blobc:
            continue
        repl = find_slice(block)
        if repl is None:
            print(f"  ! {md.name}: no reference match for block starting "
                  f"{compact(block.splitlines())[0][:60]!r}")
            continue
        out.append((m.start(2), m.end(2), repl + "\n"))
    if out:
        for start, end, repl in reversed(out):
            text = text[:start] + repl + text[end:]
        md.write_text(text)
        changed += len(out)
        print(f"  synced {len(out)} block(s) in {md.name}")

print(f"{changed} block(s) rewritten from the reference build")
