"""Per-kill rows and per-cell tallies from the dismember_odds observer campaign's outputs
(docs/spec-notes/dismemberment.md §4.1).
usage: dismember_tally.py OUT.jsonl ... [--rows] [--json FILE]

A target's life: from its first sample alive to the samples after it died (FormIDs of
deleted targets are reused). Its kill is read at the first sample dead: the last hit's JIP
flags are cleared a while after death. A part is gone if IsLimbGone says so within 10
samples of death. Fate: gone with the hit's Dismembered flag = severed, gone without it =
exploded; `expl_rise` (live explosions counted by GetNumRefs 81 going up at death) checks it."""
import json
import os
import sys
from collections import Counter, defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import observe  # noqa: E402

GONE = ["gone_%d" % k for k in range(15)]


def kills(path):
    with open(path, encoding="utf-8") as f:
        mods, obs = observe.read_samples(f, path)
    out = []
    for ob in obs:
        pf = [[tuple(x) for x in p] for p in ob["fields"].get("parts", [])]
        hf = [h[0] for h in ob["fields"].get("header", [])]
        lives = {}  # actor -> current life
        done = []
        prev_expl = None
        for i, s in enumerate(ob["samples"]):
            head = dict(zip(hf, s.get("header") or []))
            expl = head.get("n_expl")
            for raw in (s.get("groups") or {}).get("nearby", []):
                if not raw or not isinstance(raw[0], str) or not raw[0].startswith("FF"):
                    continue
                rec = observe.decode_subject(raw, pf, mods)
                aid = raw[0].split("|")[0]
                life = lives.get(aid)
                if rec.get("dead") == 0:
                    if life is None or life.get("death") is not None:
                        life = lives[aid] = {"actor": aid, "death": None, "gone": set(), "hits": 0, "last_hp": None}
                    hp = rec.get("health")
                    if life["last_hp"] is not None and hp is not None and hp < life["last_hp"]:
                        life["hits"] += 1
                    life["last_hp"] = hp
                    life["alive"] = rec
                    life["max_hp"] = max(life.get("max_hp") or 0, hp or 0)
                elif rec.get("dead") == 1 and life is not None:
                    if life["death"] is None:
                        life["death"] = rec
                        life["i"] = i
                        life["expl_rise"] = (expl or 0) - (prev_expl or 0)
                        done.append(life)
                    if i - life["i"] <= 10:
                        life["gone"] |= {k for k in range(15) if rec.get(GONE[k]) == 1}
            prev_expl = expl
        for life in done:
            d, a = life["death"], life.get("alive", {})
            gone = sorted(life["gone"])
            fate = None
            if gone:
                fate = "severed" if d.get("hit_sever") == 1 else "exploded"
            out.append({"session": ob["label"], "actor": life["actor"], "kbl": d.get("kbl"), "hloc": d.get("hloc"), "gone": gone, "fate": fate,
                        "expl_rise": life["expl_rise"], "cause": d.get("cause"),
                        "crit": d.get("hit_crit"), "sneak": d.get("hit_sneak"), "cripple": d.get("hit_cripple"),
                        "flag_sever": d.get("hit_sever"), "flag_explode": d.get("hit_explode"),
                        "hp_dmg": d.get("hit_hp"), "limb_dmg": d.get("hit_limb"), "max_hp": life.get("max_hp"),
                        "hp_before": a.get("health"), "hp_after": d.get("health"), "earlier_hits": life["hits"],
                        "goo": max(d.get("goo") or 0, d.get("goo_end") or 0),
                        "dis": max(d.get("dis") or 0, d.get("dis_end") or 0)})
    return out


def cell(r):
    k = r["hloc"] if r.get("hloc") not in (None, -1) else r["kbl"]
    part = {0: "torso", 1: "head"}.get(int(k) if isinstance(k, (int, float)) else None, f"kbl{k}")
    return r["session"], part


def main(args):
    rows = [r for p in args if p.endswith(".jsonl") for r in kills(p)]
    if "--rows" in args:
        for r in rows:
            print(json.dumps(r))
    if "--json" in args:
        with open(args[args.index("--json") + 1], "w") as f:
            json.dump(rows, f, indent=0)
    cells = defaultdict(Counter)
    for r in rows:
        c = cells[cell(r)]
        c["n"] += 1
        c["exploded"] += r["fate"] == "exploded"
        c["severed"] += r["fate"] == "severed"
        c["expl_rise"] += (r["expl_rise"] or 0) > 0
        c["crit"] += r["crit"] == 1
        c["sneak"] += r["sneak"] == 1
        c["goo"] += r["goo"] > 0
        c["dis"] += r["dis"] > 0
    for (s, part), c in sorted(cells.items()):
        print(f"{s:26} {part:7} " + " ".join(f"{k}={c[k]}" for k in ("n", "exploded", "severed", "expl_rise", "crit", "sneak", "goo", "dis")))


if __name__ == "__main__":
    main(sys.argv[1:])
