#!/usr/bin/env python3
import base64, tarfile, io, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[1]
parts_dir = root / ".bundle"
if not parts_dir.exists():
    parts_dir = pathlib.Path(sys.argv[1]) if len(sys.argv)>1 else root/".bundle"
data = "".join(p.read_text() for p in sorted(parts_dir.glob("part-*.txt")))
raw = base64.b64decode(data)
with tarfile.open(fileobj=io.BytesIO(raw), mode="r:gz") as tar:
    tar.extractall(root)
print("extracted to", root)
