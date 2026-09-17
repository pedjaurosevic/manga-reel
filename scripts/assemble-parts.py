#!/usr/bin/env python3
import json, pathlib
from collections import defaultdict
root = pathlib.Path(__file__).resolve().parents[1]
parts = root/"src"/"_parts"
man = json.loads((parts/"MANIFEST.json").read_text())
groups=defaultdict(list)
for item in man:
    groups[item["target"]].append(item)
for target, items in groups.items():
    items=sorted(items, key=lambda x: x["index"])
    text="".join((parts/it["part"]).read_text() for it in items)
    out=root/"src"/target
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    print("assembled", out, len(text))
