"""Audit actual skill-cache fields and missing sound clips; local output only.

Usage: python engine/re/skill_field_audit.py <cache> [--skill ID ...]
Based on the audit-branch tool, corrected against main's field mapping.
"""
import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

CAPSULE = {0: "aura", 1: "attack", 2: "defence", 3: "erase", 4: "status",
           5: "special", 6: "environment"}
FIELDS = [(7, 0x0A, "additional 1"), (8, 0x0C, "category (NOT sound)"),
          (10, 0x10, "interaction category"), (18, 0x20, "boss specific"),
          (36, 0x44, "self effect"), (37, 0x46, "extra button effects"),
          (40, 0x4C, "ground/air"), (42, 0x50, "unknown 2"),
          (61, 0x76, "unknown 5"), (54, 0x68, "shield pierce"),
          (55, 0x6A, "collision behaviour"), (56, 0x6C, "collision skill id")]
# s8 is a category; s12/s13 index ACTION rows, not sound ids.
SOUND_FIELDS = [(19, "s19 shot"), (20, "s20 flight"), (21, "s21 collision")]


def clip_ids(cache):
    folder = Path(cache) / "sfx/common"
    if not folder.is_dir():
        return None
    return {int(p.stem) for p in folder.iterdir()
            if p.suffix.lower() == ".wav" and p.stem.isdigit()}


def hist(skills, n, by_capsule=True):
    groups = defaultdict(Counter)
    for skill in skills:
        raw = skill["raw"]
        key = CAPSULE.get(raw[9], str(raw[9])) if by_capsule else "all"
        groups[key][raw[n]] += 1
    return groups


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cache", type=Path)
    parser.add_argument("--skill", type=int, nargs="+", default=[])
    args = parser.parse_args(argv)
    with (args.cache / "skills.json").open(encoding="utf-8") as stream:
        data = json.load(stream)
    skills = [s for s in data["skills"] if s["raw"][3]]
    print(f"{len(skills)} skill records with an id\n")
    for n, offset, name in FIELDS:
        print(f"s{n} = community 0x{offset:02X} '{name}'")
        for cap, counts in sorted(hist(skills, n).items()):
            top = ", ".join(f"{v}:{count}" for v, count in counts.most_common(8))
            print(f"   {cap:<12} {sum(counts.values()):>3} skills; value:count {top}")
        print()
    ids = clip_ids(args.cache)
    print("Sound ids named by skills vs decoded clips (s8 excluded: NOT audio)")
    if ids is None:
        print("cache/sfx/common missing: skipped")
    else:
        for n, label in SOUND_FIELDS:
            used = Counter(s["raw"][n] for s in skills if s["raw"][n] > 0)
            missing = {k: v for k, v in used.items() if k not in ids}
            print(f"   {label}: {len(used)} distinct, {len(missing)} missing: {missing}")
        rows = {row["cast_sound"] for s in skills
                for row in (s.get("action") or {}).values()
                if isinstance(row, dict) and row.get("cast_sound", 0) > 0}
        print(f"   action cast sounds: {len(rows)} distinct, missing {sorted(rows - ids)}")
        reactions = {r["sound"] for r in data.get("reactions", [])
                     if r.get("sound", 0) > 0}
        # Report what is in the cache, not a claim all rows are playable.
        print(f"   reaction sounds (all positive cached rows): {len(reactions)} distinct, "
              f"missing {sorted(reactions - ids)}")
    for sid in args.skill:
        skill = next((s for s in skills if s["id"] == sid), None)
        if skill is None:
            parser.error(f"skill {sid} not found")
        print(f"{sid} {skill['name']}: " + "  ".join(
            f"s{n}={skill['raw'][n]}" for n, _, _ in FIELDS))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
