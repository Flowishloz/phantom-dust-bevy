#!/usr/bin/env python3
"""Per-skill audit: what the original program does for each skill vs what our runtime does.

The measuring stick behind `engine/docs/audits/SKILL-BY-SKILL.md` (phase 1: measure, no fixes).

    python engine/re/skill_audit.py --run                  # probe our runtime (cargo test), compare, write the ledger
    python engine/re/skill_audit.py --ours ours.json       # compare an existing probe dump
    python engine/re/skill_audit.py --ours ours.json --skill 65 80 3   # print the cells of a few skills
    python engine/re/skill_audit.py --ours ours.json --spot-check      # the dozen already verified

Halves:
  * program side  -> `skill_audit_rules.py` (values derived from `cache/skills.json`, the action
                     and hit-reaction tables, `shotpoints.json`, through the rules read from
                     PDUWP.exe; each rule carries its address);
  * ours side     -> the Rust test `crates/pd_game/src/combat/skill_audit.rs` (test-only; it
                     drives the real `Combat` / `moves::*` code and writes JSON);
  * usage ranking -> the game's own decks (versus Quick decks in gsdata, the 402 enemy / CPU
                     decks `Data/com/deck/DECK*`, the developer decks `deckdata_debug`).

Nothing here is game data: the ledger names skills and ids and numbers derived from the
tables, never extracted assets. `cache/` and `game_files/` stay out of git.
"""

import argparse
import collections
import json
import os
import struct
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import skill_audit_rules as R  # noqa: E402

ENGINE = HERE.parent
REPO = ENGINE.parent
DEFAULT_CACHE = ENGINE / "cache"
ROSTER = range(1, 341)

# ---------------------------------------------------------------------------
# Deviations that are already decided or explained in the docs: shown as mismatches (the program
# does something else) but annotated, so a lane does not "fix" a decision.  key: (skill id or '*', 'ch.key')
KNOWN = {}


def known_note(r, c):
    """A cross that is already decided or explained in the docs (shown beside it in the ledger)."""
    kn = KNOWN.get((r.id, "%s.%s" % (c.ch, c.key)))
    if kn:
        return kn
    return ""


# ---------------------------------------------------------------------------
# Usage: the game's own decks


def read_decks_dir(path):
    """`Data/com/deck/DECK*`: 100-byte decks (u32, char name[16], i16 skills[30], u16 schools, ...)."""
    out = []
    for f in sorted(Path(path).iterdir()):
        b = f.read_bytes()
        if len(b) != 100:
            continue
        name = b[4:20].split(b"\0")[0].decode("ascii", "replace")
        skills = [x for x in struct.unpack("<30h", b[20:80]) if x != -1]
        if skills:
            out.append((f.name, name, skills))
    return out


def find_game_decks(explicit=None):
    cands = []
    if explicit:
        cands.append(Path(explicit))
    if os.environ.get("PD_GAME_FILES"):
        cands.append(Path(os.environ["PD_GAME_FILES"]))
    cands += [REPO / "game_files", REPO.parent / "Phantom Dust Rewrite" / "game_files", Path(r"..\..\..\game_files")]
    for c in cands:
        d = c / "Assets" / "Data" / "com" / "deck"
        if d.is_dir():
            return d
    return None


def table_index_map(skills_doc):
    """The program's skill index -> our skill id. A deck entry is an index into the program's skill table
    (`DAT_1404bdc50 + index x 0x90`, FUN_140262670): table index = our record + 1, 0 = the Aura Particle
    (record 0), -1 = empty. It is NOT the s3 "skill id" our game keys everything by (they agree for only
    71 of 399 records). Records appear in the file twice for ids 257 / 294; the first record of an id is the
    skill, the second is a character's dodge move (indices 750 / 751)."""
    m = {}
    first = {}
    for s in skills_doc["skills"]:
        first.setdefault(s["id"], s["record"])
    for s in skills_doc["skills"]:
        if s["record"] == first[s["id"]]:
            m[s["record"] + 1] = s["id"]
    return m


def usage_table(skills_doc, deck_dir):
    """Per skill id: copies and distinct decks by source. Sources: quick (the 8 versus Quick decks),
    cpu (the enemy / CPU decks of Data/com/deck), debug (deckdata_debug's 4 filled decks). Deck entries are
    table indices (see `table_index_map`) and are translated to our skill ids here."""
    use = collections.defaultdict(lambda: {"quick": 0, "quick_decks": 0, "cpu": 0, "cpu_decks": 0, "debug": 0, "debug_decks": 0})
    invalid = collections.Counter()
    tmap = table_index_map(skills_doc)

    def add(src, ids):
        sk = []
        for i in ids:
            if i is None or i < 0 or i == 0:
                continue  # -1 = empty; 0 = the Aura Particle used as filler, not a choice of skill
            sid = tmap.get(i)
            if sid is None:
                invalid[i] += 1  # not an index of the table (Q003's 23428 is read as empty by the program)
                continue
            sk.append(sid)
            use[sid][src] += 1
        for sid in set(sk):
            use[sid][src + "_decks"] += 1

    for q in skills_doc.get("quick_decks", []):
        add("quick", q["skills"])
    for d in skills_doc.get("decks", []):
        ids = [x for x in d["skills"] if x != -1] if "skills" in d else []
        if ids:
            add("debug", ids)
    n_cpu = 0
    if deck_dir:
        for _, _, ids in read_decks_dir(deck_dir):
            add("cpu", ids)
            n_cpu += 1
    return use, invalid, n_cpu


def index_space_evidence(skills_doc, deck_dir):
    """How the decks' numbers were shown to be table indices: how school-pure each deck is when its entries
    are read as table indices vs as our s3 skill ids. Returns a dict of counts."""
    recs = skills_doc["skills"]
    by_prog, by_id = {}, {}
    for s in recs:
        by_prog.setdefault(s["record"] + 1, s)
        by_id.setdefault(s["id"], s)
    school = lambda s: s["raw"][11]
    out = {"named": [], "quick": [], "cpu": {"decks": 0, "table_index_coherent": 0, "skill_id_coherent": 0}, "agree": sum(1 for s in recs if s["record"] + 1 == s["id"]), "records": len(recs)}

    def purity(ids, space):
        sc = collections.Counter(school(space[i]) for i in ids if i in space and i != 0 and school(space[i]))
        return (max(sc.values()) / sum(sc.values())) if sc else 0.0, len(sc)

    for d in skills_doc.get("decks", [])[:4]:
        ids = [x for x in d.get("skills", []) if x != -1]
        if ids:
            out["named"].append((d.get("name"), purity(ids, by_prog)[0], purity(ids, by_id)[0]))
    for q in skills_doc.get("quick_decks", []):
        ids = [x for x in q["skills"] if x != -1]
        out["quick"].append((q.get("name"), q.get("kind"), purity(ids, by_prog)[1], purity(ids, by_id)[1]))
    if deck_dir:
        for _, _, ids in read_decks_dir(deck_dir):
            kind = None
            out["cpu"]["decks"] += 1
            pt, ct = purity(ids, by_prog), purity(ids, by_id)
            # a deck of one school-pair is "coherent" when at most two schools show up
            out["cpu"]["table_index_coherent"] += 1 if pt[1] <= 2 else 0
            out["cpu"]["skill_id_coherent"] += 1 if ct[1] <= 2 else 0
    return out


def index_check(skills_doc, ours_doc):
    """Does OUR code build the original's built-in arsenals? `ours_doc["builtin_decks"]` is what the probe got by running
    `frontend::discovery::school_decks` (the function the versus setup lists them with) on the real `skills.json`. The program's
    decks: the four starter decks (`decks`, named) and the eight Quick decks (`quick_decks`) hold TABLE INDICES (FUN_140262670), which
    `table_index_map` turns into our s3 ids (the reference); entries that name no skill, 0 (the Aura Particle) and -1 (empty) do not
    count. A deck matches when the multiset of skills is the same. Returns None when the dump has no `builtin_decks` (an old probe)."""
    built = ours_doc.get("builtin_decks")
    if built is None:
        return None
    tmap = table_index_map(skills_doc)
    want_decks = []
    for d in skills_doc.get("decks", [])[:4]:
        ids = [x for x in d.get("skills", []) if x != -1]
        if ids and (d.get("name") or "").strip():
            want_decks.append(((d["name"] or "").strip().capitalize(), ids))
    for q in skills_doc.get("quick_decks", []):
        want_decks.append(("Quick " + (q.get("name") or "").strip(), [x for x in q["skills"] if x != -1]))
    by_name = {b["name"]: b for b in built}
    rows = []
    for name, entries in want_decks:
        want = collections.Counter(tmap[e] for e in entries if e > 0 and e in tmap)
        b = by_name.get(name)
        got = collections.Counter(x for x in (b["skills"] if b else []) if x > 0)
        same = sum(min(want[k], got[k]) for k in want)
        # read as s3 ids instead (the old, wrong reading): how many entries would name another skill
        wrong_as_ids = sum(1 for e in entries if e > 0 and e in tmap and e != tmap[e])
        rows.append({"name": name, "entries": sum(want.values()), "match": same, "extra": sum((got - want).values()), "found": b is not None, "wrong_if_ids": wrong_as_ids})
    total = sum(r["entries"] for r in rows)
    match = sum(r["match"] for r in rows)
    ok = bool(rows) and all(r["found"] and r["match"] == r["entries"] and r["extra"] == 0 for r in rows)
    return {"rows": rows, "total": total, "match": match, "ok": ok, "wrong_if_ids": sum(r["wrong_if_ids"] for r in rows)}


def index_space_text(skills_doc, deck_dir, rows, idx=None):
    ev = index_space_evidence(skills_doc, deck_dir)
    by_prog = {}
    by_id = {}
    for s in skills_doc["skills"]:
        by_prog.setdefault(s["record"] + 1, s)
        by_id.setdefault(s["id"], s)
    # how many entries of the built-in decks name another skill when read as our ids
    wrong = total = 0
    for q in skills_doc.get("quick_decks", []):
        for v in q["skills"]:
            if v > 0 and v in by_prog:
                total += 1
                wrong += 1 if by_id.get(v, {}).get("id") != by_prog[v]["id"] or (v in by_id and by_id[v] is not by_prog[v]) else 0
    for d in skills_doc.get("decks", [])[:4]:
        for v in d.get("skills", []):
            if v > 0 and v in by_prog:
                total += 1
                wrong += 1 if (v in by_id and by_id[v] is not by_prog[v]) else 0
    w = []
    w.append("## Top-priority row: which skill number each part of the game uses")
    w.append("")
    w.append("Lead from the moverec lane (live on the original): the program's skill index is the **table index** (our record + 1; index 0 = the Aura Particle), not the `id` (s3) our game keys everything by. Verified here from the data and the code:")
    w.append("")
    w.append("- **Program, READ**: a deck entry is handed to `FUN_140261a00` -> `FUN_140262670(slot, index)`, which reads `DAT_1404bdc50 + index x 0x90` directly (uses s26, record fields) -- an index into the skill table, not an s3 id. The records are 0x90 bytes from file offset 0x18 (our record r = table index r + 1). **MEASURED live** (moverec): the description text at table index 41 is Psycho Kinesis's (s3 id 18, text id 41); owned counts, deck entries and slot records all use that index.")
    w.append("- **Data, MEASURED here** (school purity of every deck; a built deck should hold one or two schools): the 4 named starter decks are %s school-pure read as table indices, but %s read as s3 ids; the 8 Quick decks hold %s schools (table index) vs %s (s3 id) against the declared 2; of the %d CPU decks, %d are 1-2 school decks read as table indices and %d read as s3 ids. The table index equals the s3 id for only %d of %d records." % (
        ", ".join("%s %.0f%%" % (n or "?", 100 * pi) for n, pi, _ in ev["named"]),
        ", ".join("%s %.0f%%" % (n or "?", 100 * si) for n, _, si in ev["named"]),
        "/".join(str(q[2]) for q in ev["quick"]), "/".join(str(q[3]) for q in ev["quick"]),
        ev["cpu"]["decks"], ev["cpu"]["table_index_coherent"], ev["cpu"]["skill_id_coherent"], ev["agree"], ev["records"]))
    w.append("")
    w.append("| part | number space it uses | right? |")
    w.append("|---|---|---|")
    w.append("| program: deck read, slot records, owned counts | table index (record + 1, 0 = Aura Particle) | (the reference) |")
    w.append("| `gsdata` Quick decks (0x33804), `deckdata_debug`, `Data/com/deck/DECK*` | table index | as the program |")
    w.append("| `skills.json` `decks` / `quick_decks` (written by `pd_convert skills`) | the raw entries: table index | faithful copy; unlabeled |")
    w.append("| `skills.json` `skills[].id` (s3), `Combat::skill(id)`, `SkillDef.id`, saved arsenals (`userdata`), the builder, capsules, every probe | s3 skill id | internally consistent; a different number from the program's |")
    if idx is None:
        w.append("| `frontend/discovery.rs` `school_decks`, `quick_decks_from_json`, `quick_decks` (the built-in arsenals) | not checked in this run (the probe dump has no `builtin_decks`) | run `python engine/re/skill_audit.py --run` |")
    elif idx["ok"]:
        w.append("| `frontend/discovery.rs` `school_decks`, `quick_decks_from_json`, `quick_decks` (the built-in arsenals) | translates the entries with record + 1 -> id (`index_to_id`) | **right (computed from our code)**: the probe runs `school_decks` on the real `skills.json` and all %d entries of the 4 starter and 8 Quick decks name the skill the program's table index names (read as s3 ids %d of them would name another skill). Fixed by the decks lane |" % (idx["total"], idx["wrong_if_ids"]))
    else:
        bad_decks = ", ".join("%s (%d of %d)" % (r["name"], r["match"], r["entries"]) for r in idx["rows"] if r["match"] != r["entries"] or r["extra"] or not r["found"])
        w.append("| `frontend/discovery.rs` `school_decks`, `quick_decks_from_json`, `quick_decks` (the built-in arsenals) | the probe runs `school_decks` on the real `skills.json` | **WRONG (computed from our code): %d of the %d entries differ from the program's table-index reading** (%s) |" % (idx["total"] - idx["match"], idx["total"], bad_decks))
    w.append("| `arsenal.rs` | no built-in decks; validates s3 ids | consistent |")
    w.append("| moverec `owned_skills.csv` (`game_index` column) | text id | equals the table index for indices <= 300; **ids 257 and 294 are keyed to the dodge records 750 / 751 (the duplicate-id trap: the skills are indices 77 and 243), and for ids 301-340 the text id (304-...) is not the table index (301-...)**: check one live take before trusting those rows |")
    w.append("| this ledger's usage ranking | translates every deck entry table index -> s3 id before counting | right (fixed from a first draft that read them as s3 ids) |")
    w.append("")
    if idx is not None and idx["ok"]:
        w.append("Lane `INDEX` is done (the decks lane fixed `frontend/discovery.rs`; verified above by running our own `school_decks`). The ledger's per-skill rows key on s3, and so do the built-in arsenals now: both agree with the program's table-index reading.")
    elif idx is not None:
        w.append("Lane `INDEX` (first in the fix lanes): translate deck entries with `record + 1 -> id` where the built-in arsenals are read (`discovery.rs`). Until then the ledger's per-skill rows are correct (they key on s3) but every statement that starts from a built-in deck is not.")
    else:
        w.append("Lane `INDEX`: not verified in this run (no `builtin_decks` in the probe dump).")
    return "\n".join(w)


def usage_score(u):
    """Rank key: distinct decks across every source, then total copies, then Quick-deck presence."""
    decks = u["quick_decks"] + u["cpu_decks"] + u["debug_decks"]
    copies = u["quick"] + u["cpu"] + u["debug"]
    return (-decks, -copies, -u["quick_decks"])


# ---------------------------------------------------------------------------
# Loading


def load_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def find_gsdata(explicit=None):
    cands = []
    if explicit:
        cands.append(Path(explicit))
    if os.environ.get("PD_GAME_FILES"):
        cands.append(Path(os.environ["PD_GAME_FILES"]))
    cands += [REPO / "game_files", REPO.parent / "Phantom Dust Rewrite" / "game_files", Path("../../../game_files")]
    for c in cands:
        f = c / "Assets" / "Data" / "gstorage" / "gsdata_en.dat"
        if f.is_file():
            return f
    return None


def load_program(cache, game_files=None):
    cache = Path(cache)
    doc = load_json(cache / "skills.json")
    sp = load_json(cache / "shotpoints.json") if (cache / "shotpoints.json").exists() else None
    hands = load_json(cache / "chars" / "pc00a.hand.json") if (cache / "chars" / "pc00a.hand.json").exists() else None
    gsf = find_gsdata(game_files)
    return R.Program(doc, sp, hands, R.GsData.open(gsf) if gsf else None)


def run_probe(out, ids=None, target_dir=None):
    """Runs the Rust probe (a test of pd_game) and returns the JSON path."""
    env = dict(os.environ)
    mingw = os.path.expandvars(r"%USERPROFILE%\.pdtools\mingw64\bin")
    cargo = str(Path.home() / ".cargo" / "bin")
    env["PATH"] = os.pathsep.join([mingw, cargo, env.get("PATH", "")])
    env["PD_SKILL_AUDIT_OUT"] = str(out)
    if ids:
        env["PD_SKILL_AUDIT_IDS"] = ",".join(str(i) for i in ids)
    else:
        env.pop("PD_SKILL_AUDIT_IDS", None)
    if target_dir:
        env["CARGO_TARGET_DIR"] = str(target_dir)
    cmd = ["cargo", "test", "-p", "pd_game", "skill_audit_dump", "--", "--nocapture"]
    print("running:", " ".join(cmd), "(in %s)" % ENGINE, file=sys.stderr)
    r = subprocess.run(cmd, cwd=ENGINE, env=env)
    if r.returncode != 0:
        raise SystemExit("the probe test failed (exit %d)" % r.returncode)
    return out


# ---------------------------------------------------------------------------
# Rows


class Row:
    def __init__(self, rec, cells, usage, rank):
        self.rec = rec
        self.cells = cells
        self.usage = usage
        self.rank = rank
        self.id = rec["id"]
        self.name = rec["name"]

    def col(self, channels):
        cs = [c for c in self.cells if c.ch in channels]
        return R.worst(cs)

    def bad(self):
        return [c for c in self.cells if c.st == R.BAD]

    def status(self):
        """Whole-skill verdict: 'bad' (any cross), 'unk', 'gap', 'ok' (every probed cell matches)."""
        sts = {c.st for c in self.cells}
        if R.BAD in sts:
            return "bad"
        if R.TAKE in sts:
            return "take"
        if R.UNK in sts:
            return "unk"
        if R.GAP in sts:
            return "gap"
        return "ok"


def build_rows(P, ours_doc, usage):
    ours = {s["id"]: s for s in ours_doc["skills"]}
    P.clips = ours_doc.get("clips", {})
    ids = [i for i in ROSTER if i in P.rec] + [0]
    ranked = sorted(ids, key=lambda i: (usage_score(usage[i]) if i else (1, 0, 0), i))
    rows = []
    for k, i in enumerate(ranked, 1):
        rec = P.rec[i] if i else P.aura_particle
        cells = R.audit_skill(P, rec, ours.get(i))
        rows.append(Row(rec, cells, usage[i], k))
    return rows


FAMILY = {
    "shell": "shell", "parabola": "lob", "fall": "fall", "crawl": "crawl", "arc": "arc", "rain_column": "rain-col", "mine": "mine",
    "around_self": "around-self", "tornado": "tornado", "object": "object", "sword": "sword", "punch": "punch", "pursuit": "pursuit",
    "hole": "hole", "ricochet": "ricochet", "rain_cloud": "rain", "instant": "instant", "none": "none", "short3": "short",
    "erase_field": "erase-field", "guard": "guard", "movement": "movement", "status_self": "self", "environment": "env", "copy": "copy",
}


def family_of(rec):
    cls = R.program_class(rec)
    cap = rec["capsule"]
    return "%s/%s" % (cap[:3], FAMILY.get(cls, cls))


# ---------------------------------------------------------------------------
# Mismatch classes


def class_of(c):
    return "%s.%s" % (c.ch, c.key.split(".")[0] if c.ch not in ("effect",) else c.key.split(".")[1] if "." in c.key else c.key)


def mismatch_classes(rows, status=R.BAD):
    groups = collections.defaultdict(list)
    for r in rows:
        seen = set()
        for c in r.cells:
            if c.st == status:
                k = "%s.%s" % (c.ch, c.key)
                if c.ch == "effect" and "." in c.key:
                    k = "effect." + c.key.split(".", 1)[1].rsplit(".", 1)[0]
                if (k, r.id) in seen:
                    continue
                seen.add((k, r.id))
                groups[k].append((r, c))
    return groups


def unsettled(rows):
    """{class: (mark, [(row, cell)])} for the UNCONFIRMED and needs-take cells."""
    out = {}
    for st in (R.TAKE, R.UNK):
        for k, lst in mismatch_classes(rows, st).items():
            out.setdefault(k, (st, []))[1].extend(lst)
    return out


# ---------------------------------------------------------------------------
# Ledger


def fmt_val(v):
    if isinstance(v, float):
        return ("%.3f" % v).rstrip("0").rstrip(".")
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(fmt_val(x) for x in v[:8]) + (", ..." if len(v) > 8 else "") + "]"
    if isinstance(v, dict):
        return "{" + ", ".join("%s: %s" % (k, fmt_val(x)) for k, x in list(v.items())[:6]) + "}"
    return str(v)


def render_ledger(rows, usage, n_cpu, invalid, ours_doc, lanes_text="", footage_text="", spot_text="", header_extra="", index_text=""):
    out = []
    w = out.append
    n = len(rows)
    sts = collections.Counter(r.status() for r in rows)
    w("# SKILL-BY-SKILL: the per-skill ledger (program vs ours)")
    w("")
    w("Generated by `engine/re/skill_audit.py` (program side: `skill_audit_rules.py`; our side: the test-only probe `crates/pd_game/src/combat/skill_audit.rs`). Phase 1 of the skill-by-skill accuracy push (DECISIONS 2026-10-01): **a measuring stick, not fixes**. Regenerate with `python engine/re/skill_audit.py --run`.")
    w("")
    w("Standing rule (Laurence, 2026-10-01): **no guessed values**. Every program value in this ledger is READ (program address / table field) or MEASURED (footage); anything else is shown as UNCONFIRMED and never counted as a match or a mismatch.")
    w("")
    w("| Mark | Meaning |")
    w("|---|---|")
    w("| %s | match: ours equals the READ / MEASURED program value |" % R.OK)
    w("| %s | mismatch with a READ / MEASURED program value |" % R.BAD)
    w("| %s | UNCONFIRMED: the program value is not settled and no recording could settle it |" % R.UNK)
    w("| %s | **needs take**: not settled from the program, but the move recorder (`engine/re/move_recorder.py`, MOVE-RECORDER.md) can measure it live on the original |" % R.TAKE)
    w("| %s | not probed: a hole in this tool (a setup the arena cannot make, a family with no probe yet), not a finding |" % R.GAP)
    w("| blank | nothing to compare for this skill in that column |")
    w("")
    w("Columns: **disp** handler picked by s43 / s63 (`FUN_140263a00`); **pay** cost, Level, Health, uses, gates; **cast** action row -> clip, fire tick, hold, air use, charge; **str** attack STR; **move** the family's own numbers (flight / area / melee / guard / crystal / self effect / range band / Add effect / erase); **react** hit-reaction rows (s57-s61); **snd** sound ids (cast / fire / travel / impact); **fx** effect number per cast row. A column shows its worst cell; the detail list below names every cross.")
    w("")
    if index_text:
        w(index_text)
        w("")
    w("## Summary")
    w("")
    w("- Skills audited: **%d** (versus roster ids 1-340 plus the built-in Aura Particle = program record 0)." % n)
    w("- **%d** skills have every probed value matching (no cross, no `?`, no `T`, no hole); **%d** have at least one mismatch; **%d** have no mismatch but need a recorded take for some value; **%d** have only UNCONFIRMED values left; **%d** have only holes in the tool (nothing wrong found, but not everything probed)." % (sts["ok"], sts["bad"], sts["take"], sts["unk"], sts["gap"]))
    cell_counts = collections.Counter(c.st for r in rows for c in r.cells)
    w("- Cells compared: %d %s, %d %s, %d T (needs take), %d ?, %d %s." % (cell_counts[R.OK], R.OK, cell_counts[R.BAD], R.BAD, cell_counts[R.TAKE], cell_counts[R.UNK], cell_counts[R.GAP], R.GAP))
    top = [r for r in rows if r.id and r.rank <= 50]
    w("- The 50 most-used skills: %d fully matching, %d with a cross, %d needing a take / UNCONFIRMED / a hole." % (sum(1 for r in top if r.status() == "ok"), sum(1 for r in top if r.status() == "bad"), sum(1 for r in top if r.status() not in ("ok", "bad"))))
    w("- Usage ranking: %d enemy / CPU decks (`Data/com/deck`), 8 versus Quick decks, 4 developer decks; the %d skills that appear in none are ranked last by id. Rank = distinct decks containing the skill, then copies, then Quick-deck presence." % (n_cpu, sum(1 for r in rows if r.id and not (r.usage["quick_decks"] + r.usage["cpu_decks"] + r.usage["debug_decks"]))))
    if invalid:
        w("- Data quirk: %d deck entries are not skill ids (%s) and were ignored." % (sum(invalid.values()), ", ".join("%d" % k for k in sorted(invalid))))
    if header_extra:
        w(header_extra)
    w("- Sources of the program values: the skill table, action table and hit-reaction table of `gsdata_en.dat`; `shotpoints.json` and the character hand path (our own exports of the program's effect markers / bone animation); the rules of `docs/PROGRAM-RE.md` with the address of each read next to every comparison (`FUN_...` in the Source column of `--json`); the new reads of this lane are marked in the lane list and in `skill_audit_rules.py` comments.")
    w("")
    w("### What a mark covers, and what it does not")
    w("")
    w("A %s in a column means every value this tool reads for that column matched. It does **not** cover: the contact geometry of a Hole against a disc guard and the steering strength of Course / About Face shots in motion (the battery resolves the block / break / Pierce / ignore-list rules of `FUN_140271760`, the sent-back / Brush stun / Add effects, and the automatic Shelters, Orb and Amulet through an incoming attack, but not where a shot turns); the steering strength of Parabola / Fall / Crawl / Arc flights in motion (their constructor numbers and the shell homing are compared, the arc re-aim and crawl turn acceleration are not); rain-drop scatter, blast growth, mine triggers beyond fuse and radius; knock-down physics and hit-reaction clips beyond the chosen row; animation, effect and shader rendering (only the effect NUMBER per cast row and whether our export has it); sound timing and mixing (only which ids); camera, lock-on and movement. Those belong to the lanes that own them." % R.OK)
    w("")

    groups = mismatch_classes(rows)
    weight = lambda r: 1 + (r.usage["quick_decks"] + r.usage["cpu_decks"] + r.usage["debug_decks"])
    w("## Top mismatch classes")
    w("")
    w("A class is one check (`channel.key`). **Skills** = how many skills fail it; **usage** = the sum of (1 + decks containing the skill) over those skills (what a fix is worth, most-used first). One shared rule or table that fails many skills is one fix.")
    w("")
    w("| class | skills | usage | what the program does | first skills (by usage rank) |")
    w("|---|---:|---:|---|---|")
    order = sorted(groups.items(), key=lambda kv: (-len(kv[1]), -sum(weight(r) for r, _ in kv[1]), kv[0]))
    for k, lst in order:
        lst = sorted(lst, key=lambda rc: rc[0].rank)
        sample = lst[0][1]
        names = ", ".join("%s (#%d)" % (r.name, r.rank) for r, _ in lst[:4])
        what = (sample.src or sample.note or "").replace("|", "/")
        w("| `%s` | %d | %d | %s | %s |" % (k, len(lst), sum(weight(r) for r, _ in lst), what[:110], names))
    w("")
    if lanes_text:
        w(lanes_text)
        w("")
    if footage_text:
        w(footage_text)
        w("")
    if spot_text:
        w(spot_text)
        w("")

    w("## The ledger")
    w("")
    w("One row per skill in usage-rank order. `decks` = distinct decks containing it (Quick / CPU / debug).")
    w("")
    cols = [name for name, _ in R.COLUMNS]
    w("| # | id | skill | family | decks | " + " | ".join(cols) + " | crosses |")
    w("|---:|---:|---|---|---:|" + "|".join(["---"] * len(cols)) + "|---:|")
    for r in rows:
        d = r.usage["quick_decks"] + r.usage["cpu_decks"] + r.usage["debug_decks"]
        dd = "%d (%d/%d/%d)" % (d, r.usage["quick_decks"], r.usage["cpu_decks"], r.usage["debug_decks"]) if d else "0"
        marks = []
        for _, chs in R.COLUMNS:
            m = r.col(chs)
            marks.append(m if m else "")
        w("| %d | %d | %s | %s | %s | " % (r.rank, r.id, r.name, family_of(r.rec), dd) + " | ".join(marks) + " | %d |" % len(r.bad()))
    w("")
    w("## Mismatch detail")
    w("")
    w("Every cross, per skill (program value READ from the tables / program; ours measured from the runtime). `[known]` = already decided or explained in the docs.")
    w("")
    for r in rows:
        bad = r.bad()
        if not bad:
            continue
        w("**#%d %s (id %d)**" % (r.rank, r.name, r.id))
        for c in bad:
            kn = known_note(r, c)
            w("- `%s.%s`: program %s, ours %s%s%s" % (c.ch, c.key, fmt_val(c.prog), fmt_val(c.ours), (" (" + c.note + ")") if c.note else "", ("  [known: %s]" % kn) if kn else ""))
        w("")
    w("## Not settled: needs a take (T) and UNCONFIRMED (?) values (not counted as match or mismatch)")
    w("")
    ug = unsettled(rows)
    w("| class | mark | skills | why it is not settled |")
    w("|---|---|---:|---|")
    for k, (mark, lst) in sorted(ug.items(), key=lambda kv: (-len(kv[1][1]), kv[0])):
        w("| `%s` | %s | %d | %s |" % (k, mark, len(lst), (lst[0][1].note or "").replace("|", "/")[:200]))
    w("")
    w("## Holes in the tool (not probed)")
    w("")
    gg = mismatch_classes(rows, R.GAP)
    w("| class | skills | what is missing |")
    w("|---|---:|---|")
    for k, lst in sorted(gg.items(), key=lambda kv: (-len(kv[1]), kv[0])):
        w("| `%s` | %d | %s |" % (k, len(lst), (lst[0][1].note or "").replace("|", "/")[:200]))
    w("")
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------
# CLI


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache", default=str(DEFAULT_CACHE), help="converted cache (default engine/cache)")
    ap.add_argument("--ours", help="probe JSON from the Rust test (see --run)")
    ap.add_argument("--run", action="store_true", help="run the Rust probe first (cargo test -p pd_game skill_audit_dump)")
    ap.add_argument("--target-dir", help="CARGO_TARGET_DIR for --run")
    ap.add_argument("--game-files", help="game_files folder (for the deck usage ranking)")
    ap.add_argument("--ledger", help="write the Markdown ledger here")
    ap.add_argument("--json", help="write every cell as JSON here")
    ap.add_argument("--skill", type=int, nargs="*", help="print the cells of these skill ids")
    ap.add_argument("--spot-check", action="store_true", help="print the dozen skills already verified")
    args = ap.parse_args(argv)
    sys.stdout.reconfigure(encoding="utf-8")

    P = load_program(args.cache, args.game_files)
    ours_path = args.ours
    if args.run:
        ours_path = ours_path or str(Path(os.environ.get("TEMP", ".")) / "skill_audit_ours.json")
        run_probe(ours_path, args.skill if args.skill else None, args.target_dir)
    if not ours_path:
        raise SystemExit("give --ours <probe.json> or --run")
    ours = load_json(ours_path)
    deck_dir = find_game_decks(args.game_files)
    usage, invalid, n_cpu = usage_table(P.doc, deck_dir)
    if deck_dir is None:
        print("warning: game_files not found: the usage ranking has no CPU decks", file=sys.stderr)
    rows = build_rows(P, ours, usage)
    by_id = {r.id: r for r in rows}
    if args.skill:
        for i in args.skill:
            r = by_id.get(i)
            if not r:
                print("no skill", i)
                continue
            print("== #%d %s (id %d) %s" % (r.rank, r.name, r.id, family_of(r.rec)))
            for c in r.cells:
                print("  %s %-34s prog=%s ours=%s %s" % (c.st, c.ch + "." + c.key, fmt_val(c.prog), fmt_val(c.ours), c.note))
    if args.spot_check:
        import skill_audit_report as REP

        print(REP.spot_text(rows))
    if args.json:
        Path(args.json).write_text(json.dumps([{"id": r.id, "name": r.name, "rank": r.rank, "cells": [c.as_dict() for c in r.cells]} for r in rows], indent=1), encoding="utf-8")
    if args.ledger:
        import skill_audit_report as REP  # the proposals and the spot check

        idx = index_check(P.doc, ours)
        text = REP.write(rows, usage, n_cpu, invalid, ours, P, index_space_text(P.doc, deck_dir, rows, idx), idx)
        Path(args.ledger).write_text(text, encoding="utf-8")
        print("wrote", args.ledger)
    elif not args.skill:
        sts = collections.Counter(r.status() for r in rows)
        print("skills %d: ok %d, bad %d, unconfirmed-only %d, gap-only %d" % (len(rows), sts["ok"], sts["bad"], sts["unk"], sts["gap"]))
        for k, lst in sorted(mismatch_classes(rows).items(), key=lambda kv: -len(kv[1]))[:25]:
            print("  %3d  %s" % (len(lst), k))


if __name__ == "__main__":
    main()
