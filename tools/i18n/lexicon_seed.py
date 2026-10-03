"""The first draft of client/i18n/glossary.ru.toml, from the translation
memory of Skadga/nethack-ru (https://github.com/Skadga/nethack-ru, NGPL):
its NetHack 5.0 tree keeps the upstream layout with the names translated
in place, so its tables pair with ours entry by entry. Run once, then the
glossary is edited by hand; this script stays to show where the first
draft came from.

    iconv -f CP1251 -t UTF-8 each file of nethack-ru into RU_TREE, then
    python3 tools/i18n/lexicon_seed.py engine/upstream RU_TREE > client/i18n/glossary.ru.toml

The Russian of an identified potion, scroll, wand, ring or spellbook is
built from nethack-ru's word for its effect, put in the genitive after the
class noun ("лечение" -> "зелье лечения"); an appearance agrees with its
class noun ("рубиновый" -> "рубиновое зелье").
"""

import sys

import lexicon_english as le
import lexicon_morph as lm
import lexicon_source as ls

CLASS_HEAD = {
    "potion": ("зелье", "n"), "scroll": ("свиток", "m"), "wand": ("палочка", "f"),
    "ring": ("кольцо", "n"), "spellbook": ("книга", "f"), "amulet": ("амулет", "m"),
    "gem": ("самоцвет", "m"), "stone": ("камень", "m"),
}
SEEN_HEAD = dict(CLASS_HEAD, spellbook=("книга заклинаний", "f"))


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


class Out:
    def __init__(self):
        self.lines = []
        self.seen = set()

    def section(self, name, comment=None):
        if self.lines:
            self.lines.append("")
        self.lines.append("[%s]" % name)
        if comment:
            self.lines.append("# " + comment)
        self.current = name

    def comment(self, text):
        self.lines.append("# " + text)

    def entry(self, en, ru, src="nethack-ru", note=None):
        key = (self.current, en)
        if key in self.seen or en is None:
            return
        self.seen.add(key)
        if not ru or ru == en and src == "nethack-ru":
            ru, src = ru or "", "todo"
        fields = ["ru = " + q(ru), "src = " + q(src)]
        if note:
            fields.append("note = " + q(note))
        self.lines.append("%s = { %s }" % (q(en), ", ".join(fields)))


def agree(adj, head, g):
    """An appearance adjective (masculine nominative) with its class noun."""
    words = adj.split(" ")
    try:
        forms = lm.adjective(words[-1])
        a = " ".join(words[:-1] + [forms[g][0]])
        return a + " " + head
    except lm.Unknown:
        return adj + " " + head


def known(o, r):
    cls = o["class"]
    if cls in ("potion", "scroll", "wand", "ring") or cls == "spellbook" and o["sn"] not in ("SPE_NOVEL", "SPE_BOOK_OF_THE_DEAD"):
        head = CLASS_HEAD[cls][0]
        try:
            return head + " " + lm.genitive(r["name"])
        except lm.Unknown:
            return head + " " + r["name"]
    if cls == "armor" and o["sn"].endswith("_DRAGON_SCALES"):
        return r["name"]
    return r["name"]


def seen(o, r):
    cls = o["class"]
    if cls in ("potion", "wand", "ring", "spellbook", "amulet") and o["sn"] not in ("SPE_NOVEL", "AMULET_OF_YENDOR", "FAKE_AMULET_OF_YENDOR"):
        head, g = SEEN_HEAD[cls]
        return agree(r["desc"], head, g)
    if cls == "gem":
        head, g = SEEN_HEAD["stone" if o["mtrl"] == "MINERAL" else "gem"]
        return agree(r["desc"], head, g)
    return r["desc"]


def main(en_root, ru_root):
    en, ru = ls.everything(en_root), ls.everything(ru_root)
    out = Out()
    out.lines += [
        "# The canonical Russian names (spec 2026-10-03-russian-localization,",
        "# decision 4): every name the engine prints, as it prints it, and its",
        "# Russian in the nominative singular (plural for things that come in",
        "# pairs). Translations of messages use these and nothing else.",
        "#",
        "#   \"English\" = { ru = \"русский\", src = \"...\", note = \"...\" }",
        "#",
        "# src: \"nethack-ru\" — from Skadga/nethack-ru (NGPL, see client/i18n/CREDITS.md);",
        "#      \"chosen\" — chosen here, the note says why when the source said",
        "#      otherwise; \"todo\" — not translated yet.",
        "# The forms of every name are in client/i18n/lexicon.ru.toml, made by",
        "# tools/i18n/lexicon_build.py; a few entries carry keys that steer it",
        "# (g, head, like, indecl, plural, proper, adjnoun, hyphen, lemma: see there).",
    ]

    out.section("monster", "monsters by class; a gendered name is its own entry")
    classes = {c["sym"]: c["desc"] for c in en["monclasses"]}
    last = None
    for m, mr in zip(en["monsters"], ru["monsters"]):
        if m["sym"] != last:
            out.comment(classes.get(m["sym"], m["sym"]))
            last = m["sym"]
        for k in ("name", "male", "female"):
            out.entry(m[k], mr[k])

    objs = [(o, r) for o, r in zip(en["objects"], ru["objects"]) if not o["sn"].startswith("GENERIC_")]
    out.section("object", "identified objects, as xname() names one")
    last = None
    for o, r in objs:
        if o["class"] != last:
            out.comment(o["class"])
            last = o["class"]
        for state, name in le.xnames(o):
            if state == "known":
                ru_name = known(o, r)
                if name.startswith("pair of "):
                    pass
                elif name.startswith("set of "):
                    pass
                out.entry(name, ru_name)
    out.comment("Samurai names (objnam.c Japanese_items)")
    for o, r in objs:
        if o["sn"] in le.JAPANESE:
            for state, name in le.xnames(o, samurai=True):
                if state == "known":
                    out.entry(name, "", "todo", "the Samurai's word for " + o["name"])

    out.section("appearance", "unidentified objects: their look, as xname() names it")
    last = None
    for o, r in objs:
        if o["class"] != last:
            out.comment(o["class"])
            last = o["class"]
        for state, name in le.xnames(o):
            if state == "seen" and o["desc"] and not name.startswith("scroll labeled "):
                out.entry(name, seen(o, r))

    out.section("label", "scroll labels (scroll labeled X)")
    for o, r in objs:
        if o["class"] == "scroll" and o["mgc"] and o["desc"]:
            out.entry(o["desc"], r["desc"], "nethack-ru" if r["desc"] != o["desc"] else "chosen",
                      None if r["desc"] != o["desc"] else "kept in Latin")

    out.section("class", "what an object is called before it is seen up close, and the word a \"called\" name hangs on")
    for word, ru_word in [("potion", "зелье"), ("scroll", "свиток"), ("wand", "палочка"), ("ring", "кольцо"),
                          ("amulet", "амулет"), ("spellbook", "книга заклинаний"), ("book", "книга"),
                          ("novel", "роман"), ("gem", "самоцвет"), ("stone", "камень")]:
        out.entry(word, ru_word, "chosen")
    simple = []
    for o, r in objs:
        if o["class"] == "armor":
            for state, name in le.xnames(o):
                if state == "called":
                    simple.append(name[: -len(" called X")])
    for s in dict.fromkeys(simple):
        out.entry(s, "", "todo")

    out.section("artifact")
    for a, ar in zip(en["artifacts"], ru["artifacts"]):
        name = a["name"][4:] if a["name"].startswith("The ") else a["name"]
        out.entry(name, ar["name"][4:] if ar["name"].startswith("The ") else ar["name"])

    out.section("role", "role names; a gendered one is its own entry")
    for ro, rr in zip(en["roles"], ru["roles"]):
        out.entry(ro["name"][0], rr["name"][0])
        out.entry(ro["name"][1], rr["name"][1])
    out.section("rank", "rank titles by role")
    for ro, rr in zip(en["roles"], ru["roles"]):
        out.comment(ro["name"][0])
        for (m, f), (rm, rf) in zip(ro["ranks"], rr["ranks"]):
            out.entry(m, rm)
            out.entry(f, rf)
    out.section("god", "a leading _ in role.c marks a goddess")
    for ro, rr in zip(en["roles"], ru["roles"]):
        for g, gr in zip(ro["gods"], rr["gods"]):
            if g:
                out.entry(g.lstrip("_"), (gr or "").lstrip("_"), note=ro["name"][0] + (", goddess" if g.startswith("_") else ""))
    out.section("race")
    for ra, rr in zip(en["races"], ru["races"]):
        out.entry(ra["noun"], rr["noun"])
        out.entry(ra["adj"], rr["adj"], note="adjective")
        out.entry(ra["coll"], rr["coll"], note="collective")
        for i, ri in zip(ra["individual"], rr["individual"]):
            out.entry(i, ri)
    out.section("alignment")
    for al, ar in zip(en["aligns"], ru["aligns"]):
        out.entry(al["adj"], ar["adj"])
        out.entry(al["noun"], ar["noun"], note="noun")
    out.section("place")
    for d, dr in zip(en["dungeons"]["dungeons"], ru["dungeons"]["dungeons"]):
        if d[:1].isupper():
            out.entry(d, dr)
    for ro, rr in zip(en["roles"], ru["roles"]):
        out.entry(ro["home"], rr["home"], note=ro["name"][0] + " quest home")
        out.entry(ro["goal"], rr["goal"], note=ro["name"][0] + " quest goal")

    out.section("terrain", "map features as farlook names them (defsym.h)")
    for t, tr in zip(en["terrain"], ru["terrain"]):
        if t["desc"] and not t["sym"].endswith("_trap") and t["sym"] not in TRAP_SYMS:
            out.entry(t["desc"], tr["desc"])
    out.section("trap")
    for t, tr in zip(en["terrain"], ru["terrain"]):
        if t["desc"] and (t["sym"].endswith("_trap") or t["sym"] in TRAP_SYMS):
            out.entry(t["desc"], tr["desc"])

    out.section("monclass", "monster classes (farlook, the / command)")
    for c, cr in zip(en["monclasses"], ru["monclasses"]):
        out.entry(c["desc"], cr["desc"])
    out.section("objclass", "object classes: the plural is a menu heading")
    for c, cr in zip(en["objclasses"], ru["objclasses"]):
        out.entry(c["name"], cr["name"])
        out.entry(c["explain"], cr["explain"])

    out.section("condition", "status line: conditions, hunger, encumbrance")
    for c, cr in zip(en["conditions"]["conditions"], ru["conditions"]["conditions"]):
        out.entry(c["txt"][0], cr["txt"][0], note=c["id"])
    for h, hr in zip(en["conditions"]["hunger"], ru["conditions"]["hunger"]):
        out.entry(h, hr)
    for e, er in zip(en["conditions"]["encumbrance"], ru["conditions"]["encumbrance"]):
        out.entry(e, er)

    out.section("skill")
    for s, sr in zip(en["skills"]["odd"][1:], ru["skills"]["odd"][1:]):
        out.entry(s, sr)
    for s, sr in zip(en["skills"]["barehands"], ru["skills"]["barehands"]):
        out.entry(s, sr)

    out.section("spell", "spells by their spellbook's name")
    for o, r in objs:
        if o["class"] == "spellbook" and o["sn"] not in ("SPE_NOVEL", "SPE_BOOK_OF_THE_DEAD", "SPE_BLANK_PAPER"):
            out.entry(o["name"], r["name"])

    out.section("shop")
    for s, sr in zip(en["shops"], ru["shops"]):
        out.entry(s["name"], sr["name"])
        out.entry(s["short"], sr["short"])

    print("\n".join(out.lines))


TRAP_SYMS = {"S_squeaky_board", "S_land_mine", "S_pit", "S_spiked_pit", "S_hole", "S_trap_door",
             "S_level_teleporter", "S_magic_portal", "S_web", "S_vibrating_square",
             "S_trapped_door", "S_trapped_chest"}

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
