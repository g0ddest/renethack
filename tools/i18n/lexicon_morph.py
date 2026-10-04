"""Russian forms for the lexicon, drafted with pymorphy3.

A name is a phrase: agreeing words (adjectives, participles) before a head
noun, the head, and a tail that never changes ("свиток телепортации": the
head declines, the genitive tail stays). pymorphy3 declines the words it
knows; a word it does not know (most monster names) declines like a model
word with the same ending ("голем" like "шлем"), picked by the ending or
named in the glossary. Every result says how it was made, so the review
can start from the guesses.
"""

import pymorphy3

CASES = ["nomn", "gent", "datv", "accs", "ablt", "loct"]
GENDERS = {"masc": "m", "femn": "f", "neut": "n"}
GRAM = {"m": "masc", "f": "femn", "n": "neut"}

_morph = None


def morph():
    global _morph
    if _morph is None:
        _morph = pymorphy3.MorphAnalyzer()
    return _morph


class Unknown(Exception):
    """No trustworthy analysis of a word."""


def _keep_case(model, word):
    """`word` with the capital `model` starts with (Медуза, not медуза)."""
    if model[:1].isupper():
        return word[:1].upper() + word[1:]
    return word


def _keep_yo(model, form):
    """pymorphy3 restores ё (пчёлы from пчела); keep it only where the
    glossary's own spelling has it, else write е as the glossary does."""
    if "ё" in model.lower():
        return form
    return form.replace("ё", "е") if "ё" in form and not _has_yo(model, form) else form


def _has_yo(model, form):
    return False


def noun_parse(word, plural=False, lemma=None):
    """The parse of `word` as a noun in the nominative (singular, or plural
    for a plural-only word) whose dictionary form is the word itself; None
    when pymorphy3 only guesses."""
    number = "plur" if plural else "sing"
    best = None
    for p in morph().parse(word):
        if p.tag.POS != "NOUN" or "nomn" not in p.tag or number not in p.tag or "Abbr" in p.tag:
            continue
        if not any(isinstance(m[0], pymorphy3.units.DictionaryAnalyzer) or
                   type(m[0]).__name__ == "HyphenatedWordsAnalyzer" for m in p.methods_stack):
            continue
        dict_form = p.normal_form.replace("ё", "е")
        if not plural and dict_form != word.lower().replace("ё", "е"):
            continue
        nom_pl = p.inflect({"nomn", "plur"})
        if plural and nom_pl and nom_pl.word.replace("ё", "е") != word.lower().replace("ё", "е"):
            continue
        if lemma and p.normal_form != lemma:
            continue
        # a common noun before a name, a place or a surname of the same
        # spelling (сабля, not the place Сабля; кнут, not Кнут)
        rank = (not any(t in p.tag for t in ("Sgtm", "Geox", "Surn", "Name", "Patr", "Orgn", "Trad")), p.score)
        if best is None or rank > best[0]:
            best = (rank, p)
    return best[1] if best else None


# A word pymorphy3 does not know declines like a model with its ending.
AUTO_MODELS = [
    ("ий", "m", "гений"), ("ей", "m", "музей"), ("ай", "m", "сарай"), ("ой", "m", "герой"),
    ("уй", "m", "буй"), ("й", "m", "герой"),
    ("ия", "f", "линия"), ("я", "f", "земля"),
    ("ка", "f", "собака"), ("га", "f", "книга"), ("ха", "f", "муха"),
    ("жа", "f", "каша"), ("ша", "f", "каша"), ("ча", "f", "туча"), ("ща", "f", "роща"),
    ("ца", "f", "птица"), ("а", "f", "рыба"),
    ("к", "m", "волк"), ("г", "m", "враг"), ("х", "m", "монах"),
    ("ж", "m", "врач"), ("ш", "m", "врач"), ("ч", "m", "врач"), ("щ", "m", "врач"),
    ("ц", "m", "кварц"),
]
INDECLINABLE_ENDINGS = ("о", "е", "и", "у", "ю", "э", "ы", "ё")


def auto_model(word, g=None):
    w = word.lower()
    if w.endswith(INDECLINABLE_ENDINGS):
        return None
    if w.endswith("ь"):
        return "тень" if g == "f" else "зверь"
    for end, mg, model in AUTO_MODELS:
        if w.endswith(end) and (g is None or g == mg or mg == "m" and end in "кгхжшчщц"):
            return model
    if w[-1:].isalpha():
        return "слон"
    return None


def _spelling(word, dict_nom, forms):
    """The dictionary writes ё where the glossary writes е (маркёр for
    маркер, Оффлёр for Оффлер): take the glossary's е in the letters of the
    stem the two share."""
    if len(dict_nom) != len(word) or dict_nom == word:
        return forms
    at = [i for i, (a, b) in enumerate(zip(dict_nom, word)) if a == "ё" and b == "е"]
    if not at or any(a != b for i, (a, b) in enumerate(zip(dict_nom, word)) if i not in at):
        return forms
    out = []
    for f in forms:
        f = list(f)
        for i in at:
            if i < len(f) and f[i] == "ё":
                f[i] = "е"
        out.append("".join(f))
    return out


def _model_forms(model):
    """(sg, pl, g) of a model word straight from pymorphy3."""
    p = noun_parse(model)
    if p is None:
        raise Unknown("model " + model)
    sg = [p.inflect({c, "sing"}).word for c in CASES]
    pl = [p.inflect({c, "plur"}).word for c in CASES]
    return sg, pl, GENDERS.get(p.tag.gender, "m")


def _common_prefix(words):
    s = words[0]
    for w in words[1:]:
        while not w.startswith(s):
            s = s[:-1]
    return s


def like_forms(word, model):
    """The forms of `word` built on the endings of `model`."""
    sg_m, pl_m, g = _model_forms(model)
    stem_m = _common_prefix([f.replace("ё", "е") for f in sg_m + pl_m])
    end_nom = sg_m[0][len(stem_m):]
    if not word.lower().endswith(end_nom):
        raise Unknown("%s does not end like %s" % (word, model))
    stem = word[: len(word) - len(end_nom)] if end_nom else word

    def build(forms):
        return [stem + f[len(stem_m):] for f in forms]

    sg, pl = build(sg_m), build(pl_m)
    # after a velar or a hushing consonant "ы" is spelled "и"
    def spell(f):
        for i in range(len(stem), len(f)):
            if f[i] == "ы" and i > 0 and f[i - 1].lower() in "кгхжшчщ":
                f = f[:i] + "и" + f[i + 1 :]
        return f
    return [spell(f) for f in sg], [spell(f) for f in pl], g


def noun_forms(word, anim, plural_only=False, like=None, g=None, hyphen="all", lemma=None):
    """(sg, pl, gender, how) of one noun: six singular and six plural forms
    with the accusative by animacy; "how" is "pymorphy3", "like:<model>" or
    "indeclinable". A hyphenated noun whose parts are nouns declines both
    (пчела-убийца, мышь-вампир), and takes the first part's gender; with
    hyphen="last" only the last part declines (Тот-Амон)."""
    if "-" in word and not like:
        parts = word.split("-")
        if hyphen == "last":
            sg, pl, g0, how = noun_forms(parts[-1], anim, plural_only, None, g)
            lead = "-".join(parts[:-1]) + "-"
            return ([lead + f for f in sg] if sg else None,
                    [lead + f for f in pl] if pl else None, g or g0, how)
        if all(noun_parse(x, plural_only) is not None for x in parts):
            forms = [noun_forms(x, anim, plural_only, None, None) for x in parts]
            sg = None if plural_only else ["-".join(f[0][i] for f in forms) for i in range(6)]
            pl = ["-".join(f[1][i] for f in forms) for i in range(6)] if all(f[1] for f in forms) else None
            return sg, pl, g or forms[0][2], "+".join(sorted(set(f[3] for f in forms)))
    if plural_only:
        p = noun_parse(word, plural=True, lemma=lemma)
        if p is None:
            raise Unknown(word)
        if "Fixd" in p.tag:
            return None, [word] * 6, "p", "indeclinable"
        pl = [_keep_case(word, p.inflect({c, "plur"}).word) for c in CASES]
        pl[3] = pl[1] if anim else pl[0]
        return None, pl, "p", "pymorphy3"
    p = None if like else noun_parse(word)
    if p is not None and "Fixd" in p.tag:
        return [word] * 6, [word] * 6, g or GENDERS.get(p.tag.gender, "m"), "indeclinable"
    if p is not None:
        g0 = GENDERS.get(p.tag.gender, "m")
        sg = [p.inflect({c, "sing"}) for c in CASES]
        pls = [p.inflect({c, "plur"}) for c in CASES]
        sg = [_keep_case(word, f.word) for f in sg]
        pl = None if None in pls else [_keep_case(word, f.word) for f in pls]
        sg, pl = _spelling(word, sg[0], sg), (_spelling(word, sg[0], pl) if pl else None)
        how = "pymorphy3"
    else:
        model = like or auto_model(word, g)
        if model is None:
            return [word] * 6, [word] * 6, g or "m", "indeclinable"
        sg, pl, g0 = like_forms(word, model)
        how = "like:" + model
    g = g or g0
    # the accusative: a noun in -а/-я has its own (собаку, мужчину); a
    # masculine one takes the genitive when animate, else the nominative,
    # whatever animacy the dictionary gave the word
    if not sg[0].lower().endswith(("а", "я")):
        if g0 == "m":
            sg[3] = sg[1] if anim else sg[0]
        elif g0 == "n":
            sg[3] = sg[0]
    if pl is not None:
        pl[3] = pl[1] if anim else pl[0]
    return sg, pl, g, how


# where pymorphy3 reads a word two ways, the adjective we mean
PREFER = {"больший": "большой"}


def adj_parse(word):
    """The parse of `word` as a full adjective or participle in the
    nominative, in whatever gender or number it is written."""
    cands = [p for p in morph().parse(word) if p.tag.POS in ("ADJF", "PRTF") and "nomn" in p.tag]
    if not cands:
        raise Unknown(word)
    for p in cands:
        if PREFER.get(p.normal_form) and any(q.normal_form == PREFER[p.normal_form] for q in cands):
            continue
        return p
    return cands[0]


ADJ_ENDINGS = ("ый", "ий", "ой", "ая", "яя", "ое", "ее", "ые", "ие")


def _unknown_adjective(word):
    """A word that ends like an adjective and that pymorphy3 knows neither
    as an adjective nor as a noun in its dictionary form."""
    if not word.endswith(ADJ_ENDINGS):
        return False
    for p in morph().parse(word):
        if p.tag.POS in ("ADJF", "PRTF") and "nomn" in p.tag:
            return False
        if p.tag.POS == "NOUN" and "nomn" in p.tag and p.normal_form == word.lower():
            return False
    return True


ADJ_MODELS = [
    ("мий", "волчий"),
    ("кой", "дорогой"), ("гой", "дорогой"), ("ой", "голубой"),
    ("ший", "хороший"), ("жий", "свежий"), ("чий", "горячий"), ("щий", "будущий"),
    ("кий", "громкий"), ("гий", "строгий"), ("хий", "тихий"), ("ний", "синий"), ("ий", "громкий"),
    ("ый", "новый"),
]


def adj_forms(word, gender, number="sing"):
    """The six case forms of an adjective agreeing with a noun of that
    gender (masc/femn/neut) or plural; the accusative is the inanimate one.
    An adjective pymorphy3 does not know, or one it reads as a form of
    another word (высший as высокий), declines like a model. An adverb
    (очень) stays."""
    if _is_adverb(word) and not _is_agreeing(word):
        return [word] * 6
    try:
        p = adj_parse(word)
    except Unknown:
        return _adj_like(word, gender, number)
    own = p.inflect({"nomn"} | ({"plur"} if "plur" in p.tag else {p.tag.gender, "sing"}))
    if own is None or own.word != word.lower():
        return _adj_like(word, gender, number)
    grams = {"plur"} if number == "plur" else {gender, "sing"}
    forms = []
    for c in CASES:
        f = p.inflect({c} | grams)
        if f is None:
            raise Unknown(word)
        forms.append(_keep_case(word, f.word))
    if number == "plur" or gender in ("masc", "neut"):
        forms[3] = forms[0]
    return forms


def _adj_like(word, gender, number, model=None):
    """An adjective in the masculine nominative declined like a model."""
    for end, m in ([("", model)] if model else ADJ_MODELS):
        if word.endswith(end):
            model = m
            mf = adj_forms(model, gender, number)
            stem_m = _common_prefix(mf + [model])
            stem = word[: len(word) - (len(model) - len(stem_m))]
            return [stem + f[len(stem_m):] for f in mf]
    raise Unknown(word)


def adjective_like(word, model):
    """All forms of an adjective declined like a model adjective."""
    return {
        "m": _adj_like(word, "masc", "sing", model), "f": _adj_like(word, "femn", "sing", model),
        "n": _adj_like(word, "neut", "sing", model), "pl": _adj_like(word, None, "plur", model),
    }


def adjective(word):
    """All forms of an adjective written in the masculine nominative
    ("благословенный"); in a multiword one the last word declines ("очень
    ржавый"), the words before it stay."""
    words = word.split(" ")
    # the adjective is the first word that reads as one: adverbs before
    # it stay (очень ржавый), and so does a tail after it (жаренный во фритюре)
    at = next((i for i, w in enumerate(words) if _is_agreeing(w) or _unknown_adjective(w)), len(words) - 1)
    lead, adj, tail = " ".join(words[:at]), words[at], " ".join(words[at + 1:])

    def join(forms):
        return [" ".join(x for x in (lead, f, tail) if x) for f in forms]

    return {
        "m": join(adj_forms(adj, "masc")),
        "f": join(adj_forms(adj, "femn")),
        "n": join(adj_forms(adj, "neut")),
        "pl": join(adj_forms(adj, None, "plur")),
    }


def _is_agreeing(word):
    return any(p.tag.POS in ("ADJF", "PRTF") and "nomn" in p.tag for p in morph().parse(word))


def _noun_first(word):
    ps = morph().parse(word)
    return bool(ps) and ps[0].tag.POS == "NOUN" and "nomn" in ps[0].tag and ps[0].score > 0.5


def split_phrase(text, head=None):
    """(modifiers, head, tail) of a nominative noun phrase: the head is the
    first word after the adjectives in front, or the word given."""
    words = text.split(" ")
    if head is not None:
        hw = head.split(" ")
        for i in range(len(words)):
            if words[i : i + len(hw)] == hw:
                return words[:i], head, words[i + len(hw):]
        raise Unknown("head %r not in %r" % (head, text))
    for i, w in enumerate(words):
        if i + 1 < len(words) and (_is_agreeing(w) or _unknown_adjective(w) or _is_adverb(w)):
            continue
        return words[:i], w, words[i + 1 :]
    raise Unknown(text)


def _is_adverb(word):
    """An adverb before an adjective (очень тяжёлое): it stays as it is."""
    ps = morph().parse(word)
    return bool(ps) and ps[0].tag.POS == "ADVB"


def phrase(text, anim, g=None, head=None, plural_only=False, like=None, indeclinable=False, proper=False,
           adjnoun=False, hyphen="all", lemma=None):
    """The forms of a noun phrase: {"g", "sg", "pl", "few", "how"}; "few"
    is the nominative after 2, 3 and 4: the head in the genitive singular,
    the adjectives in the genitive plural (the nominative plural before a
    feminine noun)."""
    if indeclinable:
        out = {"g": g or "m", "how": "indeclinable"}
        if plural_only:
            out["g"] = "p"
            out["pl"] = [text] * 6
        else:
            out["sg"] = [text] * 6
            out["pl"] = [text] * 6
            out["few"] = text
        return out
    mods, h, tail = split_phrase(text, head)
    if adjnoun:
        # a noun made of an adjective (Тёмный, гончая) declines as one
        sg_h = adj_forms(h, GRAM[g or "m"], "sing")
        pl_h = adj_forms(h, None, "plur")
        if anim:
            if (g or "m") == "m":
                sg_h[3] = sg_h[1]
            pl_h[3] = pl_h[1]
        hg, how = g or "m", "adjective"
    elif " " in h:
        # a head of several words declines word by word (лорд Карнарвон)
        parts = [noun_forms(w, anim, plural_only, None, g, hyphen) for w in h.split(" ")]
        sg_h = None if plural_only else [" ".join(p[0][i] for p in parts) for i in range(6)]
        pl_h = [" ".join(p[1][i] for p in parts) for i in range(6)] if all(p[1] for p in parts) else None
        hg = g or parts[0][2]
        how = "+".join(sorted(set(p[3] for p in parts)))
    else:
        sg_h, pl_h, hg, how = noun_forms(h, anim, plural_only, like, g, hyphen, lemma)
    g = g or hg
    if proper and sg_h is not None:
        pl_h = None
    tail_s = (" " + " ".join(tail)) if tail else ""
    out = {"g": g, "how": how}
    if sg_h is not None:
        ms = [adj_forms(m, GRAM[g], "sing") for m in mods]
        sg = []
        for i in range(6):
            src = 1 if i == 3 and anim and g == "m" else i
            sg.append(" ".join([mf[src] for mf in ms] + [sg_h[i]]) + tail_s)
        out["sg"] = sg
        loc = None if anim or adjnoun or how != "pymorphy3" or " " in h or "-" in h else locative(h)
        if loc:
            out["loc"] = " ".join([mf[5] for mf in ms] + [loc]) + tail_s
    if pl_h is not None:
        mp = [adj_forms(m, None, "plur") for m in mods]
        pl = []
        for i in range(6):
            src = 1 if i == 3 and anim else i
            pl.append(" ".join([mf[src] for mf in mp] + [pl_h[i]]) + tail_s)
        out["pl"] = pl
        if sg_h is not None:
            # a head that declines like an adjective (гончая) agrees with the
            # numeral as the adjectives do: 2 адские гончие, 2 Тёмных
            if adjnoun or _adjectival(sg_h):
                head_few = pl_h[0] if g == "f" else pl_h[1]
            else:
                head_few = sg_h[1]
            out["few"] = " ".join([mf[0] if g == "f" else mf[1] for mf in mp] + [head_few]) + tail_s
    return out


# second locatives OpenCorpora has that the written language does not
# take for a place: на щите, в роге, в супе, в форте (в цвету is in bloom)
NO_LOC2 = {"хвост", "цвет", "час", "вид", "дом", "род", "счёт", "остров", "щит", "рог", "болт", "форт", "суп",
           "корень", "язык", "свет", "сок", "крем", "чай"}


def locative(word):
    """The second locative of a noun, after в and на of a place (пол: на
    полу, лёд: во льду), or None when it takes the prepositional."""
    p = noun_parse(word)
    if p is None or "Fixd" in p.tag or p.normal_form in NO_LOC2:
        return None
    f = p.inflect({"loc2", "sing"})
    if f is None or "loc2" not in f.tag:
        return None
    prep = p.inflect({"loct", "sing"})
    if prep is not None and prep.word == f.word:
        return None
    return _keep_case(word, f.word)


def _adjectival(sg):
    """Forms of a noun that declines like an adjective: гончая, гончей."""
    nom, gen = sg[0].lower(), sg[1].lower()
    return nom.endswith(ADJ_ENDINGS) and gen.endswith(("ого", "его", "ой", "ей"))


def genitive(text, plural_only=False):
    """A noun phrase in the genitive, for building "свиток <чего>"."""
    f = phrase(text, anim=False, plural_only=plural_only)
    return f["pl" if plural_only else "sg"][1]
