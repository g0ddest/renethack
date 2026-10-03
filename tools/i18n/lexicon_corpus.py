"""The names the game can print, for the name parser's tests: every object
in each of its looks and states as doname() writes them, and every monster
as x_monnam() and its family write it.

    python3 tools/i18n/lexicon_corpus.py engine/upstream > client/rust/nh-i18n/tests/data/names.en.txt

Each line is a kind ("o" an object, "m" a monster), a tab and the name.
The combinations doname() allows are many; each object gets every prefix
and every suffix that applies to it at least once, in turn (a rusty one, a
cursed one, a greased one...), rather than all their products.
"""

import itertools
import sys

import lexicon_english as le
import lexicon_source as ls

MATERIALS = ["LIQUID", "WAX", "VEGGY", "FLESH", "PAPER", "CLOTH", "LEATHER", "WOOD", "BONE",
             "DRAGON_HIDE", "IRON", "METAL", "COPPER", "SILVER", "GOLD", "PLATINUM", "MITHRIL",
             "PLASTIC", "GLASS", "GEMSTONE", "MINERAL"]


def mat(o):
    m = o["mtrl"]
    return MATERIALS.index(m) if m in MATERIALS else -1


def organic(o):
    return 0 <= mat(o) <= MATERIALS.index("WOOD") and o["mtrl"] != "LIQUID"


def weptool(o):
    return o["class"] == "tool" and o["sub"] != "P_NONE"


def rustprone(o):
    return o["mtrl"] == "IRON"


def corrodeable(o):
    return o["mtrl"] in ("IRON", "COPPER")


def crackable(o):
    return o["mtrl"] == "GLASS" and o["class"] == "armor"


def flammable(o):
    return (organic(o) or o["mtrl"] == "PLASTIC") and "CANDLE" not in o["sn"]


def rottable(o):
    return organic(o) or o["mtrl"] == "DRAGON_HIDE"


def erosion_matters(o):
    return o["class"] in ("weapon", "armor", "ball", "chain") or weptool(o)


def erosions(o):
    """The erosion and erodeproof words add_erosion_words() can give it."""
    if not erosion_matters(o):
        return []
    out = []
    first = "rusty" if rustprone(o) else "cracked" if crackable(o) else "burnt"
    if rustprone(o) or crackable(o) or flammable(o):
        out += [first, "very " + first, "thoroughly " + first]
    if corrodeable(o) or rottable(o):
        second = "corroded" if corrodeable(o) else "rotted"
        out += [second, "very " + second, "thoroughly " + second]
    proof = ("fixed" if o["sn"] == "CRYSKNIFE" else "rustproof" if rustprone(o) else
             "corrodeproof" if corrodeable(o) else "fireproof" if flammable(o) else
             "tempered" if crackable(o) else "rotproof" if rottable(o) else None)
    if proof:
        out.append(proof)
    return out


def ammo(o):
    return o["class"] == "weapon" and str(o["sub"]).startswith("-P_") and o["sub"] in ("-P_BOW", "-P_SLING", "-P_CROSSBOW")


def missile(o):
    return o["class"] == "weapon" and o["sub"] in ("-P_DART", "-P_SHURIKEN", "-P_BOOMERANG")


def poisonable(o):
    return ammo(o) or missile(o) and o["sub"] != "-P_BOOMERANG"


def statuses(o):
    """The parenthesized states doname() can append to it."""
    cls, sn, sub = o["class"], o["sn"], o["sub"]
    out = ["unpaid, 15 zorkmids", "for sale, 1 zorkmid", "for sale, 22 zorkmids, 7 aum", "no charge"]
    if cls == "weapon" or weptool(o):
        if o["big"]:
            out += ["weapon in hands", "weapon in claws", "tethered to hands" if sn == "AKLYS" else "weapon in pedipalps"]
        else:
            out += ["weapon in right hand", "weapon in left hand", "wielded in left hand",
                    "weapon in right claw", "weapon in left tentacle", "weapon in right hand, brightly lit",
                    "weapon in right hand, glimmering light blue"]
        if sn == "AKLYS":
            out += ["tethered to right hand"]
        out += ["wielded", "alternate weapon; not wielded", "alternate weapons; not wielded",
                "in quiver", "at the ready"]
    if cls == "armor":
        out += ["being worn", "being doffed", "being donned", "being worn, brilliantly lit"]
        if sub == "ARM_GLOVES":
            out.append("being worn; slippery")
        if sn.endswith("_DRAGON_SCALES"):
            out.append("embedded in your skin")
    if cls == "amulet":
        out.append("being worn")
    if cls == "ring":
        out += ["on left hand", "on right paw", "in quiver pouch"]
    if cls == "wand" or (cls == "tool" and o["chrg"]):
        out += ["0:5", "1:-1"]
    if sn in ("OIL_LAMP", "MAGIC_LAMP", "BRASS_LANTERN", "TALLOW_CANDLE", "WAX_CANDLE", "POT_OIL"):
        out.append("lit")
    if sn in ("BLINDFOLD", "TOWEL", "LENSES", "SADDLE"):
        out.append("being worn")
    if sn == "LEASH":
        out += ["attached to your little dog", "attached to Fido"]
    if sn == "CANDELABRUM_OF_INVOCATION":
        out += ["1 of 7 candle attached", "3 of 7 candles attached", "7 of 7 candles, lit"]
    if sn == "EGG":
        out.append("laid by you")
    if cls == "ball":
        out.append("chained to you")
    if cls == "chain":
        out.append("attached to you")
    if cls in ("gem", "coin", "wand", "amulet"):
        out.append("in quiver pouch")
    return out


def prefixes(o):
    """The words doname() can put before it (with "a"/"an" or a count)."""
    cls, sn = o["class"], o["sn"]
    out = [[], ["blessed"], ["uncursed"], ["cursed"], ["greased"]]
    out += [[e] for e in erosions(o)]
    if cls in ("weapon", "armor") or weptool(o) or (cls == "ring" and o["chrg"]):
        out += [["+0"], ["+1"], ["-2"], ["uncursed", "+3"], ["blessed", "rustproof", "+2"] if rustprone(o) else ["blessed", "+2"]]
    if poisonable(o):
        out += [["poisoned"], ["uncursed", "poisoned", "+0"]]
    if sn in ("LARGE_BOX", "CHEST"):
        out += [["locked"], ["unlocked"], ["broken"], ["trapped"], ["empty"], ["uncursed", "trapped", "locked"]]
    if sn in ("ICE_BOX", "SACK", "OILSKIN_SACK", "BAG_OF_HOLDING", "BAG_OF_TRICKS", "HORN_OF_PLENTY", "STATUE"):
        out.append(["empty"])
    if cls == "food":
        out += [["partly eaten"], ["uncursed", "partly eaten"]]
    if sn in ("TALLOW_CANDLE", "WAX_CANDLE"):
        out.append(["partly used"])
    if cls == "potion":
        out += [["diluted"], ["uncursed", "diluted"]]
    if sn == "TOWEL":
        out += [["moist"], ["wet"]]
    if sn == "BOULDER":
        out.append(["next"])
    if sn in ("LARGE_BOX", "CHEST", "SACK", "BAG_OF_HOLDING"):
        out.append(["unpaid"])
    return out


def object_lines(o, turn, samurai=False):
    """Each look of `o` with each of its prefixes once, a few of its
    suffixes (a different few for each object: `turn`) and a name."""
    out = []
    sts = statuses(o)
    for state, name in le.xnames(o, samurai):
        called = name.replace("called X", "called healing")
        plural = le.makeplural(called)
        for i, words in enumerate(prefixes(o)):
            pre = " ".join(words + [called])
            out.append([le.an(pre), "the " + pre, pre, "%d %s" % (2 + i, " ".join(words + [plural]))][(i + turn) % 4])
        for j in range(3):
            out.append(le.an(called) + " (" + sts[(turn * 3 + j) % len(sts)] + ")")
        if state in ("seen", "known") and o["class"] != "coin":
            out.append(le.an(called) + " named Bob" if turn % 2 else "2 %s named Lucky Bob" % plural)
    return out


def containers(objs):
    out = []
    for o in objs:
        if o["ctnr"]:
            for state, name in le.xnames(o):
                out += [le.an(name) + " containing 1 item", "an uncursed %s containing 12 items" % name,
                        "the contents of your " + name, "an unpaid %s and its contents" % name]
    return out


def monster_names(mons):
    out = []
    for i, m in enumerate(mons):
        for n in le.monster_names(m):
            if m["pname"]:
                out += [n]
                continue
            if m["unique"]:
                out += ["the " + n, "The " + n]
                continue
            more = ["the invisible " + n, "a saddled " + n, "the poor " + n, "the falling " + n,
                    n + " called Fido", "the angry " + n, "Your " + n, "The " + n]
            out += ["the " + n, le.an(n), "your " + n, le.makeplural(n), more[i % 8], more[(i + 3) % 8]]
    return out


def monster_objects(mons):
    out = []
    sizes = ["", "small ", "medium ", "large ", "very large "]
    varieties = ["soup made from", "french fried", "pickled", "boiled", "smoked", "dried",
                 "deep fried", "szechuan", "broiled", "stir fried", "sauteed", "candied", "pureed"]
    for i, m in enumerate(mons):
        n = m["name"]
        if m["pname"] or m["unique"]:
            poss = n + "'s" if m["pname"] else "the " + n + "'s"
            the = n if m["pname"] else "the " + n
            out += [poss + " corpse", [poss + " partly eaten corpse", poss + " 2 uncursed corpses",
                                       "a statue of " + the, "a historic statue of " + the][i % 4]]
            continue
        forms = [le.an(n + " corpse"), le.an("uncursed partly eaten " + n + " corpse"),
                 "2 %s corpses" % n, le.an(n + " egg"), "3 uncursed %s eggs" % n,
                 "a statue of " + le.an(n), "an uncursed figurine of %s named Bob" % le.an(n),
                 "a tin of %s meat" % n, "a tin of " + n, "2 tins of %s meat" % n,
                 "a rotten tin of %s meat" % n, "a homemade tin of %s meat" % n,
                 "a tin of %s %s meat" % (varieties[i % 13], n)]
        out += [forms[0], forms[i % len(forms)], forms[(i + 5) % len(forms)], forms[(i + 9) % len(forms)]]
        for g in (m["male"], m["female"]):
            if g:
                out += ["a statue of " + le.an(g), "a figurine of " + le.an(g)]
    for glob in ["gray ooze", "brown pudding", "green slime", "black pudding"]:
        for s in sizes:
            out.append("a %sglob of %s" % (s, glob))
    out += ["a tin of spinach", "an uncursed empty tin", "3 tins of spinach", "an egg", "2 eggs",
            "a statue", "a figurine", "the 2nd arrow", "the 3rd dagger", "the 11th dart",
            "a tin of soup made from lichen", "a tin of pickled lichen"]
    return out


def artifacts(arts, objs):
    by_sn = {o["sn"]: o for o in objs}
    out = []
    for a in arts:
        o = by_sn.get(a["otyp"])
        if not o:
            continue
        name = a["name"]
        bare = name[4:] if name.startswith("The ") else name
        base = o["name"]
        if o["sub"] in ("ARM_GLOVES", "ARM_BOOTS") or o["sn"] == "LENSES":
            base = "pair of " + base
        lowered = "the " + bare if name.startswith("The ") else name
        out += [le.an(base) + " named " + lowered, "the " + bare, "the blessed rustproof +3 " + bare,
                "the uncursed " + bare + " (weapon in hand)"]
    return out


def shopkeepers(shks):
    out = []
    for i, s in enumerate(shks[::7]):
        n = s["name"]
        out += [n, n + " the invisible shopkeeper", "the angry " + n]
        out.append("o\t%s's long sword" % n)
    return out


def priests(roles):
    out = []
    gods = [g.lstrip("_") for r in roles for g in r["gods"] if g] + ["Moloch"]
    for g in gods:
        out += ["the priest of " + g, "the priestess of " + g, "the high priest of " + g,
                "the invisible renegade high priestess of " + g, "the guardian Angel of " + g,
                "a renegade Angel of " + g, "the grand poohbah of " + g]
    return out


def main(root):
    src = ls.everything(root)
    objs = le.real_objects(src["objects"])
    lines = []
    for turn, o in enumerate(objs):
        lines += ["o\t" + x for x in object_lines(o, turn)]
        if o["sn"] in le.JAPANESE:
            lines += ["o\t" + x for x in object_lines(o, turn + 1, samurai=True)]
    lines += ["o\t" + x for x in containers(objs)]
    lines += ["o\t" + x for x in monster_objects(src["monsters"])]
    lines += ["o\t" + x for x in artifacts(src["artifacts"], objs)]
    lines += ["o\t" + x for x in ["a very heavy iron ball (chained to you)", "13 gold pieces",
                                    "some gold pieces", "a gold piece", "your 2 daggers",
                                    "the newt's long sword", "Fido's +0 dagger",
                                    "some uncursed partly eaten food rations"]]
    lines += ["m\t" + x for x in monster_names(src["monsters"])]
    for x in shopkeepers(src["shopkeepers"]):
        lines.append(x if x.startswith("o\t") else "m\t" + x)
    lines += ["m\t" + x for x in priests(src["roles"])]
    lines += ["m\t" + x for x in ["it", "It", "someone", "Someone", "something", "you", "You",
                                    "itself", "himself", "herself", "themselves",
                                    "Bob's ghost", "Fido", "the invisible Fido", "your Fido",
                                    "the stripling", "Bob the Valkyrie", "Kaz the invisible stripling"]]
    seen = set()
    out = []
    for line in lines:
        if line not in seen:
            seen.add(line)
            out.append(line)
    print("# Names the engine prints, made by tools/i18n/lexicon_corpus.py from engine/upstream.")
    print("# kind <tab> name: o an object (doname, xname...), m a monster (x_monnam...)")
    print("\n".join(out))


if __name__ == "__main__":
    main(sys.argv[1])
