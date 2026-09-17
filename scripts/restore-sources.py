#!/usr/bin/env python3
import base64, json, pathlib
root = pathlib.Path(__file__).resolve().parents[1]
man = json.loads((root/".srcb64/manifest.json").read_text())
from collections import defaultdict
groups=defaultdict(list)
for item in man:
    groups[item["rel"]].append(item)
for rel, items in groups.items():
    items=sorted(items, key=lambda x: x["index"])
    b64="".join((root/".srcb64"/it["part"]).read_text() for it in items)
    path=root/rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(base64.b64decode(b64))
    print("restored", path, path.stat().st_size)
