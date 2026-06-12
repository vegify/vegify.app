#!/usr/bin/env python3
"""Inline design/figma-import/text-overrides.json into plugin.src.js -> code.js."""
import json, pathlib

here = pathlib.Path(__file__).resolve().parent
data_path = here.parents[1] / "design" / "figma-import" / "text-overrides.json"

records = json.loads(data_path.read_text())
payload = [
    {k: r[k] for k in ("page", "container", "instance", "master", "textOverrides")}
    for r in records
    if any(o["value"] != o["default"] for o in r["textOverrides"])
]

src = (here / "plugin.src.js").read_text()
out = src.replace("/*__DATA__*/", json.dumps(payload, ensure_ascii=False), 1)
(here / "code.js").write_text(out)
print(f"code.js written: {len(payload)} records, {len(out) / 1024:.0f} KB")
