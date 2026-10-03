"""How the engine builds the English name of a thing: a port of the parts
of objnam.c (xname, doname's prefixes, makeplural, just_an) that the
lexicon and the name corpus need. It works on the tables of
lexicon_source.py; nothing here knows Russian.
"""

import re

VOWELS = "aeiouAEIOU"

# ---------------------------------------------------------------- plural

ONE_OFF = [
    ("child", "children"), ("cubus", "cubi"), ("culus", "culi"), ("Cyclops", "Cyclopes"),
    ("djinni", "djinn"), ("erinys", "erinyes"), ("foot", "feet"), ("fungus", "fungi"),
    ("goose", "geese"), ("knife", "knives"), ("labrum", "labra"), ("louse", "lice"),
    ("mouse", "mice"), ("mumak", "mumakil"), ("nemesis", "nemeses"), ("ovum", "ova"),
    ("ox", "oxen"), ("passerby", "passersby"), ("rtex", "rtices"), ("serum", "sera"),
    ("staff", "staves"), ("tooth", "teeth"),
]
AS_IS = [
    "boots", "shoes", "gloves", "lenses", "scales", "eyes", "gauntlets", "iron bars",
    "bison", "deer", "elk", "fish", "fowl", "tuna", "yaki", "-hai", "krill", "manes",
    "moose", "ninja", "sheep", "ronin", "roshi", "shito", "tengu", "ki-rin", "Nazgul",
    "gunyoki", "piranha", "samurai", "shuriken", "haggis", "Bordeaux",
]
COMPOUNDS = [" of ", " labeled ", " called ", " named ", " above", " versus ", " from ",
             " in ", " on ", " a la ", " with", " de ", " d'", " du ", " au ", "-in-", "-at-"]
NO_MEN = ["albu", "antihu", "anti", "ata", "auto", "bildungsro", "cai", "cay", "ceru", "corner",
          "decu", "des", "dura", "fir", "hanu", "het", "infrahu", "inhu", "nonhu", "otto", "out",
          "prehu", "protohu", "subhu", "superhu", "talis", "unhu", "sha", "hu", "un", "le", "re",
          "so", "to", "at", "a"]
CH_K = ["monarch", "poch", "tech", "mech", "stomach", "psych", "amphibrach", "anarch",
        "atriarch", "azedarach", "broch", "gastrotrich", "isopach", "loch", "oligarch",
        "peritrich", "sandarach", "sumach", "symposiarch"]


def _ends(s, suffix):
    return s.lower().endswith(suffix.lower())


def _badman_plural(base):
    if len(base) < 4:
        return False
    for p in NO_MEN:
        spot = len(base) - (len(p) + 3)
        if spot >= 0 and base[spot:spot + len(p)].lower() == p and (spot == 0 or base[spot - 1] == " "):
            return True
    return False


def _compound(s):
    best = None
    for i in range(len(s)):
        if s[i] not in " -":
            continue
        for c in COMPOUNDS:
            if s[i:i + len(c)].lower() == c:
                return i
    return best


def makeplural(old):
    """objnam.c makeplural() for the names objects and monsters have."""
    s = old.lstrip(" ")
    pronouns = {"he": "they", "she": "they", "it": "they", "him": "them", "her": "them", "his": "their", "its": "their"}
    if s.lower() in pronouns:
        p = pronouns[s.lower()]
        return p.capitalize() if s[0].isupper() else p
    if s.lower().startswith("pair of "):
        return s
    excess = ""
    at = _compound(s)
    if at is not None:
        s, excess = s[:at], s[at:]
    s = s.rstrip(" ")
    n = len(s)
    if n == 1 or not s[-1].isalpha():
        return s + "'s" + excess
    # singplur_lookup(to_plural)
    for a in AS_IS + ["ae", "eaux", "matzot"]:
        if _ends(s, a):
            return s + excess
    if n > 5 and _ends(s, "craft"):
        return s + excess
    if s.lower() in ("slice", "mongoose"):
        return s + "s" + excess
    if n > 2 and _ends(s, "ox") and not (n > 5 and _ends(s, "muskox")):
        return s + "es" + excess
    if n > 2 and _ends(s, "man") and _badman_plural(s):
        return s + "s" + excess
    for sing, plur in ONE_OFF:
        if _ends(s, plur):
            return s + excess
        if _ends(s, sing):
            return s[: n - len(sing)] + plur + excess
    if (n == 2 and s.lower() == "ya") or (n >= 3 and _ends(s, " ya")):
        return s + excess
    if n >= 3 and _ends(s, "man") and not _badman_plural(s):
        return s[:-2] + "en" + excess
    if s[-1].lower() == "f":
        lo = s[-2].lower()
        if n >= 3 and _ends(s, "erf"):
            pass
        elif lo in "lr" or lo in VOWELS:
            return s[:-1] + "ves" + excess
    if n >= 3 and _ends(s, "ium"):
        return s[:-2] + "a" + excess
    if (n >= 4 and _ends(s, "alga")) or (n >= 5 and (_ends(s, "hypha") or _ends(s, "larva"))) \
            or (n >= 6 and _ends(s, "amoeba")) or (n >= 8 and _ends(s, "vertebra")):
        return s + "e" + excess
    if n > 3 and _ends(s, "us") and not ((n >= 5 and _ends(s, "lotus")) or (n >= 6 and _ends(s, "wumpus"))):
        return s[:-2] + "i" + excess
    if n >= 3 and _ends(s, "sis"):
        return s[:-2] + "es" + excess
    if n >= 3 and _ends(s, "eau") and not _ends(s, "bureau"):
        return s + "x" + excess
    if n >= 6 and (_ends(s, "matzoh") or _ends(s, "matzah")):
        return s[:-2] + "ot" + excess
    if n >= 5 and (_ends(s, "matzo") or _ends(s, "matza")):
        return s[:-1] + "ot" + excess
    lo = s[-1].lower()
    if n >= 5 and (_ends(s, "dex") or _ends(s, "dix") or _ends(s, "tex")) and not _ends(s, "index"):
        return s[:-2] + "ices" + excess
    if lo in "zxs" or (n >= 2 and lo == "h" and s[-2].lower() in "cs"
                       and not (n >= 4 and s[-2].lower() == "c" and any(_ends(s, k) for k in CH_K))) \
            or (n >= 4 and _ends(s, "ato")) or (n >= 5 and _ends(s, "dingo")):
        return s + "es" + excess
    if lo == "y" and s[-2].lower() not in VOWELS:
        return s[:-1] + "ies" + excess
    return s + "s" + excess


def just_an(s):
    """The article doname() and an() put in front: "a ", "an " or ""."""
    c0 = s[:1].lower()
    if len(s) == 1 or s[1:2] == " ":
        return "an " if c0 in "aefhilmnosx" else "a "
    if s.lower().startswith("the ") or s.lower() in ("molten lava", "iron bars", "ice"):
        return ""
    lo = s.lower()
    if (c0 in VOWELS.lower()
            and not (lo.startswith("one") and (len(s) == 3 or s[3] in "-_ "))
            and not lo.startswith("eu") and not lo.startswith("uke") and not lo.startswith("ukulele")
            and not lo.startswith("unicorn") and not lo.startswith("uranium") and not lo.startswith("useful")) \
            or (c0 == "x" and s[1:2].lower() not in VOWELS.lower()):
        return "an "
    return "a "


def an(s):
    return just_an(s) + s


# ---------------------------------------------------------------- objects

JAPANESE = {
    "SHORT_SWORD": "wakizashi", "BROADSWORD": "ninja-to", "FLAIL": "nunchaku", "GLAIVE": "naginata",
    "LOCK_PICK": "osaku", "WOODEN_HARP": "koto", "MAGIC_HARP": "magic koto", "KNIFE": "shito",
    "PLATE_MAIL": "tanko", "HELMET": "kabuto", "LEATHER_GLOVES": "yugake", "FOOD_RATION": "gunyoki",
    "POT_BOOZE": "sake",
}

GEM_PLAIN = {"DILITHIUM_CRYSTAL", "RUBY", "DIAMOND", "SAPPHIRE", "BLACK_OPAL", "EMERALD", "OPAL"}


def gem_stone(o):
    """GemStone(): gems whose identified name takes " stone"."""
    return o["sn"] == "FLINT" or (o["mtrl"] == "GEMSTONE" and o["sn"] not in GEM_PLAIN)


def real_objects(objs):
    """The objects the game can make: no generic placeholders."""
    return [o for o in objs if not o["sn"].startswith("GENERIC_")]


def armor_simple(o):
    """armor_simple_name() before and after the type is known: the word an
    unidentified armor's "called" name hangs on."""
    sub, name, desc = o["sub"], o["name"], o["desc"] or ""
    if sub == "ARM_SUIT":
        if name.endswith("dragon scale mail"):
            return ["dragon mail"]
        if name.endswith("dragon scales"):
            return ["dragon scales"]
        if name.endswith(" mail"):
            return ["mail"]
        if name.endswith(" jacket"):
            return ["jacket"]
        return ["suit"]
    if sub == "ARM_CLOAK":
        return {"ROBE": ["robe"], "MUMMY_WRAPPING": ["wrapping"], "ALCHEMY_SMOCK": ["apron", "smock"]}.get(o["sn"], ["cloak"])
    if sub == "ARM_HELM":
        soft = o["mtrl"] in ("CLOTH", "LEATHER", "WOOD") or o["sn"] in ("ELVEN_LEATHER_HELM", "FEDORA", "CORNUTHAUM", "DUNCE_CAP")
        return ["hat" if soft else "helm"]
    if sub == "ARM_GLOVES":
        return ["gauntlets" if "gauntlets" in name or "gauntlets" in desc else "gloves"]
    if sub == "ARM_BOOTS":
        return ["shoes" if "shoes" in name or "shoes" in desc else "boots"]
    if sub == "ARM_SHIELD":
        return ["silver shield", "smooth shield"] if o["sn"] == "SHIELD_OF_REFLECTION" else ["shield"]
    return ["shirt"]


def xnames(o, samurai=False):
    """Every xname() a single object of this type can have, as
    (state, name): state is "unseen" (!dknown), "seen" (appearance known,
    type not), "called" (with "X" standing for the player's name) or
    "known". Prefixes that depend on the object's own state (poisoned,
    diluted, holy, moist, partly eaten...) are not here."""
    cls, sn = o["class"], o["sn"]
    actual = o["name"]
    dn = o["desc"]
    if samurai and sn in JAPANESE:
        actual = JAPANESE[sn]
    if samurai and sn in ("WOODEN_HARP", "MAGIC_HARP"):
        dn = "koto"
    if actual is None:
        actual = "strange object"
    if dn is None:
        dn = actual
    out = []

    def add(state, name):
        if (state, name) not in out:
            out.append((state, name))

    if cls == "amulet":
        add("unseen", "amulet")
        if sn in ("AMULET_OF_YENDOR", "FAKE_AMULET_OF_YENDOR"):
            add("seen", dn)
            add("known", actual)
            return out
        add("seen", dn + " amulet")
        add("called", "amulet called X")
        add("known", actual)
    elif cls in ("weapon", "venom", "tool"):
        pre = "pair of " if sn == "LENSES" else ""
        add("unseen", pre + dn)
        add("seen", pre + dn)
        add("called", pre + dn + " called X")
        add("known", pre + actual)
    elif cls == "armor":
        if sn.endswith("_DRAGON_SCALES"):
            add("known", "set of " + actual)
            return out
        pre = "pair of " if o["sub"] in ("ARM_BOOTS", "ARM_GLOVES") else ""
        if o["sub"] == "ARM_SHIELD" and sn in ("ELVEN_SHIELD", "URUK_HAI_SHIELD", "ORCISH_SHIELD"):
            add("unseen", "shield")
        elif sn == "SHIELD_OF_REFLECTION":
            add("unseen", "smooth shield")
        else:
            add("unseen", pre + dn)
        add("seen", pre + dn)
        for simple in armor_simple(o):
            add("called", pre + simple + " called X")
        add("known", pre + actual)
    elif cls == "food":
        add("known", actual)
    elif cls in ("coin", "chain", "rock", "ball", "illobj"):
        add("known", actual)
    elif cls == "potion":
        add("unseen", "potion")
        add("seen", dn + " potion")
        add("called", "potion called X")
        add("known", "potion of " + actual)
        if sn == "POT_WATER":
            add("known", "potion of holy water")
            add("known", "potion of unholy water")
    elif cls == "scroll":
        add("unseen", "scroll")
        if o["mgc"]:
            add("seen", "scroll labeled " + dn)
        else:
            add("seen", dn + " scroll")
        add("called", "scroll called X")
        if o["name"]:
            add("known", "scroll of " + actual)
    elif cls == "wand":
        add("unseen", "wand")
        add("seen", dn + " wand")
        add("called", "wand called X")
        if o["name"]:
            add("known", "wand of " + actual)
    elif cls == "spellbook":
        if sn == "SPE_NOVEL":
            add("unseen", "book")
            add("seen", dn + " book")
            add("called", "novel called X")
            add("known", actual)
        else:
            add("unseen", "spellbook")
            add("seen", dn + " spellbook")
            add("called", "spellbook called X")
            add("known", actual if sn == "SPE_BOOK_OF_THE_DEAD" else "spellbook of " + actual)
    elif cls == "ring":
        add("unseen", "ring")
        add("seen", dn + " ring")
        add("called", "ring called X")
        add("known", "ring of " + actual)
    elif cls == "gem":
        rock = "stone" if o["mtrl"] == "MINERAL" else "gem"
        add("unseen", rock)
        if o["desc"]:
            add("seen", dn + " " + rock)
        add("called", rock + " called X")
        add("known", actual + (" stone" if gem_stone(o) else ""))
    return out


# ---------------------------------------------------------------- monsters


def monster_names(m):
    """The names a monster type prints: neutral, and the gendered ones."""
    out = []
    for n in (m["name"], m["male"], m["female"]):
        if n and n not in out:
            out.append(n)
    return out
