#!/usr/bin/env python3
"""Writes the tables of the records of the compiled show format into docs/format.md, from crates/core/src/format.rs.

    python3 tools/format-tables.py          # rewrite the tables
    python3 tools/format-tables.py --check  # fail if they are not up to date (CI)
"""

import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parent.parent
source = (root / "crates/core/src/format.rs").read_text()
document = root / "docs/format.md"

SIZES = {"u8": 1, "u16": 2, "i16": 2, "u32": 4, "i32": 4, "i64": 8, "f32": 4}

records = {}
order = []

for match in re.finditer(r"record!\(\s*((?:\s*///[^\n]*\n)*)\s*(\w+) \{(.*?)\n    \}\n\);", source, re.S):
    doc = " ".join(line.strip()[3:].strip() for line in match.group(1).strip().splitlines() if line.strip())
    fields = []
    comment = []

    for line in match.group(3).splitlines():
        line = line.strip()

        if line.startswith("///"):
            comment.append(line[3:].strip())
        elif line.startswith("//") or not line:
            continue
        else:
            name, kind = line.rstrip(",").split(": ", 1)
            fields.append((name, kind, " ".join(comment)))
            comment = []

    records[match.group(2)] = (doc, fields)
    order.append(match.group(2))


def size_of(kind):
    if kind in SIZES:
        return SIZES[kind]
    if kind.startswith("List<"):
        return 8
    return sum(size_of(field_kind) for _, field_kind, _ in records[kind][1])


def kind_text(kind):
    if kind.startswith("List<"):
        inner = kind[5:-1]
        return f"list of {'[' + inner + '](#' + inner.lower() + ')' if inner in records else '`' + inner + '`'}"
    if kind in records:
        return f"[{kind}](#{kind.lower()})"
    return f"`{kind}`"


lines = []

for name in order:
    doc, fields = records[name]
    lines.append(f"### {name}")
    lines.append("")
    lines.append(f"{doc + ' ' if doc else ''}{size_of(name)} bytes.")
    lines.append("")
    lines.append("| Offset | Field | Type | Holds |")
    lines.append("| -----: | ----- | ---- | ----- |")
    offset = 0

    for field, kind, comment in fields:
        lines.append(f"| {offset} | `{field}` | {kind_text(kind)} | {comment} |")
        offset += size_of(kind)

    lines.append("")

# (prettier leaves the tables as they are written here)
tables = "<!-- prettier-ignore-start -->\n\n" + "\n".join(lines).rstrip() + "\n\n<!-- prettier-ignore-end -->\n"
text = document.read_text()
start = "<!-- records:start -->\n"
end = "<!-- records:end -->"
before, rest = text.split(start, 1)
_, after = rest.split(end, 1)
updated = before + start + "\n" + tables + "\n" + end + after

if "--check" in sys.argv:
    if updated != text:
        sys.exit("docs/format.md is not up to date with crates/core/src/format.rs: run python3 tools/format-tables.py")
else:
    document.write_text(updated)
