"""client/i18n/glossary.ru.toml -> client/i18n/lexicon.ru.toml: every
glossary name with all its Russian forms.

    python3 -m venv VENV && VENV/bin/pip install -r tools/i18n/requirements.txt
    VENV/bin/python tools/i18n/lexicon_build.py

A lexicon entry mirrors its glossary entry, section and English key:

    [monster."newt"]            a noun phrase
    g = "m"                     m, f, n, or p (plural only: сапоги)
    anim = true                 the accusative of an animate noun is its genitive
    sg = [six cases]            nom, gen, dat, acc, ins, prep
    pl = [six cases]
    few = "тритона"             the nominative after 2, 3, 4 (2 тритона)
    unit = "pair"               counted in pairs: 2 пары сапог
    src = "pymorphy3"           how the forms were made (below)

    [adjective."blessed"]       an adjective: m, f, n, pl, six cases each
                                (the inanimate accusative)
    [label."ZELGO MER"]         fixed = "..." : words that never change

src says how the forms were made: "pymorphy3"; "like:<word>" (declined on
the endings of a model word pymorphy3 knows); "indeclinable"; "fixed"; and
"hand" — corrected by a person. The build keeps every "hand" entry of the
current lexicon as it is, so a correction survives the next build; to
redo one, delete it or change its src.

A glossary entry can steer the draft with optional keys: g (gender), anim,
head (the words that decline, when they are not the first noun: "лорд
Карнарвон"), like (a model word), indecl (true: the phrase never changes),
plural (true: the phrase has no singular), proper (true: no plural),
adjnoun (true: the head declines like an adjective: Тёмный), hyphen
("last": only the last part of a hyphenated head declines: Тот-Амон),
lemma (the dictionary word a plural head belongs to: свитки -> свиток).
"""

import os
import sys
import tomllib

import lexicon_morph as lm

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
GLOSSARY = os.path.join(ROOT, "client", "i18n", "glossary.ru.toml")
LEXICON = os.path.join(ROOT, "client", "i18n", "lexicon.ru.toml")

# how a section's names are declined
ANIMATE = {"monster", "role", "rank", "god"}
PROPER = {"artifact", "god", "place"}
ADJECTIVES = {"adjective", "color", "gender"}
FIXED = {"label", "status", "heading", "condition", "monclass"}
LINKS = {"called", "named", "labeled"}
# a few words in other sections are adjectives
ADJECTIVE_KEYS = {
    ("race", "elven"), ("race", "dwarven"), ("race", "gnomish"), ("race", "orcish"),
    ("alignment", "lawful"), ("alignment", "neutral"), ("alignment", "chaotic"), ("alignment", "unaligned"),
}
# fixed in the objclass section: the "X or Y" descriptions
FIXED_KEYS = {("objclass", k) for k in (
    "suit or piece of armor", "useful item (pick-axe, key, lamp...)", "gem or rock", "boulder or statue",
)} | {("monster", k) for k in ("it", "someone", "something", "you", "himself", "herself", "itself", "themselves")}
FIXED_KEYS |= {("skill", k) for k in ("Restricted", "Unskilled", "Basic", "Skilled", "Expert", "Master", "Grand Master")}
FIXED_KEYS |= {("term", "Elbereth"), ("spell", "clerical")}


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def key(s):
    return q(s)


def arr(forms):
    return "[" + ", ".join(q(f) for f in forms) + "]"


def build_noun(section, en, g_entry):
    ru = g_entry["ru"]
    anim = g_entry.get("anim", section in ANIMATE)
    plural = g_entry.get("plural", False)
    unit = None
    if en.startswith("pair of "):
        plural, unit = True, "pair"
    elif en.startswith("set of "):
        unit = "set"
    if not plural and not g_entry.get("indecl") and _plural_only(ru):
        plural = True
    proper = g_entry.get("proper", section in PROPER or (section == "monster" and ru[:1].isupper()))
    f = lm.phrase(
        ru, anim, g=g_entry.get("g"), head=g_entry.get("head"), plural_only=plural,
        like=g_entry.get("like"), indeclinable=g_entry.get("indecl", False), proper=proper,
        adjnoun=g_entry.get("adjnoun", False), hyphen=g_entry.get("hyphen", "all"), lemma=g_entry.get("lemma"),
    )
    out = {"g": f["g"], "anim": anim, "src": f["how"]}
    for k in ("sg", "pl", "few"):
        if k in f:
            out[k] = f[k]
    if unit:
        out["unit"] = unit
    return out


def _plural_only(ru):
    """A phrase whose head is a plural noun with no singular in its own
    right (сапоги, очки, Подземелья Рока): its first noun is plural."""
    try:
        mods, head, tail = lm.split_phrase(ru)
    except lm.Unknown:
        return False
    for p in lm.morph().parse(head):
        if p.tag.POS == "NOUN" and "nomn" in p.tag:
            return "plur" in p.tag and "sing" not in p.tag and (
                "Pltm" in p.tag or p.inflect({"nomn", "sing"}) is None
                or p.inflect({"nomn", "sing"}).word != head.lower())
    return False


def build_adjective(ru, g_entry):
    if " " not in ru and not ru.endswith(("ый", "ий", "ой")) and not any(
            p.tag.POS == "ADJF" for p in lm.morph().parse(ru)):
        return {"fixed": ru, "src": "fixed"}
    if g_entry.get("like"):
        forms, how = lm.adjective_like(ru, g_entry["like"]), "like:" + g_entry["like"]
    else:
        forms, how = lm.adjective(ru), "pymorphy3"
    return {"m": forms["m"], "f": forms["f"], "n": forms["n"], "pl": forms["pl"], "src": how}


def build(section, en, g_entry):
    ru = g_entry["ru"]
    if (section, en) in FIXED_KEYS or section in FIXED or (section == "part" and en in LINKS) \
            or (section == "objclass" and " or " in en):
        return {"fixed": ru, "src": "fixed"}
    if section in ADJECTIVES or (section, en) in ADJECTIVE_KEYS:
        return build_adjective(ru, g_entry)
    return build_noun(section, en, g_entry)


def write_entry(lines, section, en, e):
    lines.append("")
    lines.append("[%s.%s]" % (section, key(en)))
    for k in ("fixed", "g", "anim", "unit", "sg", "pl", "few", "m", "f", "n"):
        if k not in e:
            continue
        v = e[k]
        if k == "anim":
            if v:
                lines.append("anim = true")
        elif isinstance(v, list):
            lines.append("%s = %s" % (k, arr(v)))
        else:
            lines.append("%s = %s" % (k, q(v)))
    lines.append("src = %s" % q(e["src"]))


def main():
    with open(GLOSSARY, "rb") as f:
        glossary = tomllib.load(f)
    kept = {}
    if os.path.exists(LEXICON):
        with open(LEXICON, "rb") as f:
            old = tomllib.load(f)
        for section, entries in old.items():
            for en, e in entries.items():
                if e.get("src") == "hand":
                    kept[(section, en)] = e
    lines = [
        "# The forms of every name in client/i18n/glossary.ru.toml, made by",
        "# tools/i18n/lexicon_build.py (pymorphy3 drafts, rules, hand corrections).",
        "# Do not edit a drafted entry here: fix the glossary, or set src = \"hand\"",
        "# on the entry you correct and the next build keeps it. The format is",
        "# described in tools/i18n/lexicon_build.py.",
    ]
    failed = []
    stats = {}
    for section, entries in glossary.items():
        for en, g_entry in entries.items():
            if (section, en) in kept:
                e = kept[(section, en)]
            else:
                try:
                    e = build(section, en, g_entry)
                except lm.Unknown as err:
                    failed.append((section, en, g_entry["ru"], str(err)))
                    e = {"fixed": g_entry["ru"], "src": "failed"}
            stats[e["src"].split(":")[0]] = stats.get(e["src"].split(":")[0], 0) + 1
            write_entry(lines, section, en, e)
    with open(LEXICON, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print("entries by how they were made:", stats, file=sys.stderr)
    for fl in failed:
        print("FAILED", *fl, file=sys.stderr)


if __name__ == "__main__":
    main()
