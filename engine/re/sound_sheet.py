"""Build a local listening sheet from decoded common sounds (never commit output).

Default selection excludes actual skill/reaction sound references and known
system ids. Explicit --ids overrides exclusions for targeted re-audition.
The template defaults NEXT TO the output, not inside tracked source code.
"""
import argparse
import array
import hashlib
import html
import json
import os
import re
import sys
import wave
from pathlib import Path

LABELS = ["", "UI", "capsule", "aura", "guard", "hit", "step", "voice", "ambience", "unknown"]
DEFAULT_RANGES = "44-300,355-500"
# Already labelled/under investigation on main; do not ask Laurence again.
KNOWN_IDS = {1, 4, 30, 35, 52, 53, 54, 55, 85, 92}


def parse_ranges(text):
    result = set()
    for part in re.split(r"[,\s]+", text.strip()):
        if part:
            first, _, last = part.partition("-")
            result.update(range(int(first), int(last or first) + 1))
    return result


def referenced_ids(skills_json):
    try:
        with open(skills_json, encoding="utf-8") as stream:
            data = json.load(stream)
    except (OSError, ValueError):
        return None
    found = set()

    def add(value):
        if isinstance(value, int) and not isinstance(value, bool) and value > 0:
            found.add(value)

    for skill in data.get("skills", []):
        raw = skill.get("raw", [])
        for n in (19, 20, 21):
            if len(raw) > n:
                add(raw[n])
            add(skill.get(f"s{n}"))
        for row in (skill.get("action") or {}).values():
            if isinstance(row, dict):
                add(row.get("cast_sound"))
    for row in data.get("reactions", []):
        add(row.get("sound"))
    # Never recursively collect raw action words, s8, s13, voice_group or
    # s70_73: those integers are NOT common-bank sound references.
    return found


def wav_info(path):
    try:
        with wave.open(str(path), "rb") as wav:
            n, rate, width = wav.getnframes(), wav.getframerate(), wav.getsampwidth()
            raw = wav.readframes(n)
        if width == 2:
            samples = array.array("h")
            samples.frombytes(raw[:len(raw) // 2 * 2])
            if sys.byteorder == "big":
                samples.byteswap()
            peak = max((abs(x) for x in samples), default=0) / 32768.0
        elif width == 1:
            peak = max((abs(x - 128) for x in raw), default=0) / 128.0
        else:
            peak = None
        return n / rate if rate else 0.0, peak
    except (OSError, wave.Error, EOFError):
        return None, None


PAGE = """<!doctype html><meta charset=utf-8><title>Sound listening sheet</title>
<style>body{font:14px sans-serif;margin:16px}td,th{padding:4px}input{width:16em}</style>
<h1>Sound listening sheet</h1><p>%(n)d distinct clips. Duplicate ids are listed together.</p>
<button onclick="exportLabels()">Export labels.json</button>
<table><tr><th>ids<th>player<th>seconds<th>peak<th>label<th>note</tr>%(rows)s</table>
<script>function exportLabels(){var out={};document.querySelectorAll('tr[data-id]').forEach(function(r){
var s=r.querySelector('select').value,t=r.querySelector('input').value;
if(s||t)r.dataset.ids.split(',').forEach(function(id){out[id]={label:s,note:t};});});
var b=new Blob([JSON.stringify(out,null,2)],{type:'application/json'});
var a=document.createElement('a');a.href=URL.createObjectURL(b);a.download='labels.json';
document.body.appendChild(a);a.click();a.remove();}</script>"""


def build(cache, ids_text, out, template=None):
    folder = Path(cache) / "sfx/common"
    if not folder.is_dir():
        print(f"no such folder: {folder}", file=sys.stderr)
        return 1
    available = {int(p.stem): p for p in folder.iterdir()
                 if p.suffix.lower() == ".wav" and p.stem.isdigit()}
    if ids_text:
        wanted = parse_ranges(ids_text)
    else:
        references = referenced_ids(Path(cache) / "skills.json")
        wanted = (parse_ranges(DEFAULT_RANGES) if references is None
                  else set(available) - references) - KNOWN_IDS
        # Exclude byte-identical aliases of known sounds as well.
        known_hashes = {hashlib.sha256(available[i].read_bytes()).digest()
                        for i in KNOWN_IDS if i in available}
        wanted = {i for i in wanted & set(available)
                  if hashlib.sha256(available[i].read_bytes()).digest() not in known_hashes}
    grouped = {}
    for i in sorted(wanted & set(available)):
        digest = hashlib.sha256(available[i].read_bytes()).digest()
        grouped.setdefault(digest, []).append(i)
    output = Path(out).resolve()
    rows, labels = [], {}
    for aliases in grouped.values():
        i = aliases[0]
        wav = available[i].resolve()
        duration, peak = wav_info(wav)
        try:
            src = Path(os.path.relpath(wav, output.parent)).as_posix()
        except ValueError:  # Windows paths on different drives
            src = wav.as_uri()
        opts = "".join(f"<option>{label}</option>" for label in LABELS)
        duration = f"{duration:.2f}" if duration is not None else "?"
        peak = f"{peak:.2f}" if peak is not None else "?"
        ids = ",".join(map(str, aliases))
        rows.append(f'<tr data-id="{i}" data-ids="{ids}"><td>{ids}<td>'
                    f'<audio controls preload=none src="{html.escape(src, quote=True)}"></audio>'
                    f'<td>{duration}<td>{peak}<td><select>{opts}</select><td><input type=text></tr>')
        labels.update({str(alias): {"label": "", "note": ""} for alias in aliases})
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(PAGE % {"n": len(rows), "rows": "\n".join(rows)}, encoding="utf-8")
    tpl = Path(template) if template else output.with_name("sound_labels_template.json")
    tpl.parent.mkdir(parents=True, exist_ok=True)
    tpl.write_text(json.dumps(labels, indent=1), encoding="utf-8")
    print(f"{len(rows)} distinct clips ({len(labels)} ids) -> {output}; template {tpl}")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", default="engine/cache")
    parser.add_argument("--ids")
    parser.add_argument("--out", default="engine/shots/sound_sheet.html")
    parser.add_argument("--template")
    args = parser.parse_args(argv)
    return build(args.cache, args.ids, args.out, args.template)


if __name__ == "__main__":
    raise SystemExit(main())
