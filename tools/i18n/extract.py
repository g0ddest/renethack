#!/usr/bin/env python3
"""Extract every English text the engine can show into the catalog the
client translates by: client/i18n/catalog.en.json.

    python3 tools/i18n/extract.py            # write the catalog
    python3 tools/i18n/extract.py --check    # fail if it is out of date

Sources (engine/upstream, read only):

- src/*.c: every pline-family call (pline, You, Your, You_feel, You_hear,
  You_see, You_cant, pline_The, There, Norep, verbalize, custompline,
  urgent_pline, pline_mon...), with the prefix the call adds ("You ",
  "The "...) folded into the format, as vpline sees it; the Sprintf /
  Snprintf / Strcpy / Strcat calls that build text; menu items, text
  window lines, questions (yn_function, getlin...), getobj verbs,
  occupations, enlightenment lines, death reasons;
- dat/: rumors, oracles, epitaphs, engravings, hallucinatory monster
  names, the quest texts of quest.lua (their %p, %r... codes become %s
  placeholders), the level messages and engravings of the other .lua
  files (the tutorial among them). Not dat/tribute or dat/data.base.

An argument that can only be one of a few literals ("swap places with" or
"frighten"), a verb conjugated for its subject (vtense, Tobjnam...) or a
local buffer filled by Sprintf/Strcpy/Strcat before the call gives the
format a derived entry per value, so whole sentences can be translated;
the generic entry then says `"expanded": true`.

Each entry: a stable id (a hash of the format), the format (printf style:
%s, %d, %c, %ld...; a literal % is %%), what shows it (`uses`), what each
conversion is (`args`: monster, species, object, word, number, char,
text, quest:<code>...), the entries it derives from (`from`, their ids)
and the call sites (file:line function call(arguments)). Entries are
sorted by their first site, one per line.
"""

import argparse
import hashlib
import itertools
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import clex  # noqa: E402
import datfiles  # noqa: E402
from clex import match_close, source_text, split_args  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
UPSTREAM = os.path.join(ROOT, "engine", "upstream")
OUT = os.path.join(ROOT, "client", "i18n", "catalog.en.json")
CATALOG_FORMAT = 1
ENGINE = "NetHack-5.0.0_Released"
# a call with more combinations of literal arguments keeps placeholders
# for its most varied arguments
MAX_DERIVED = 96
# what a buffer can hold: its writes with the appends after them
MAX_HELD = 4 * MAX_DERIVED
# how deep buffers built from buffers are followed
MAX_DEPTH = 3
# a text with more Strcats after its Sprintf is a list: only its pieces
MAX_APPENDS = 3

# ---------------------------------------------------------------- calls

# The pline family: name -> (index of the format, prefixes vpline sees).
# Several prefixes give an entry each (You_feel says "You dream that you
# feel " while the hero is unconscious).
PLINE = {
    "pline": (0, [""]),
    "pline_dir": (1, [""]),
    "pline_xy": (2, [""]),
    "pline_mon": (1, [""]),
    "custompline": (1, [""]),
    "urgent_pline": (0, [""]),
    "Norep": (0, [""]),
    "You": (0, ["You "]),
    "Your": (0, ["Your "]),
    "You_cant": (0, ["You can't "]),
    "pline_The": (0, ["The "]),
    "There": (0, ["There "]),
    "You_feel": (0, ["You feel ", "You dream that you feel "]),
    "You_hear": (0, ["You hear ", "You barely hear ", "You dream that you hear "]),
    "You_see": (0, ["You see ", "You sense ", "You dream that you see "]),
    "verbalize": (0, ['"']),
    "raw_printf": (0, [""]),
    "livelog_printf": (1, [""]),
}
PLINE_USE = {"raw_printf": "raw", "livelog_printf": "livelog"}
# one-argument shorthands: X1(cstr) is X("%s", cstr)
PLINE1 = {"pline1": "pline", "You1": "You", "Your1": "Your",
          "verbalize1": "verbalize", "You_hear1": "You_hear"}
# Calls whose arguments are shown text that is not a format:
# name -> [(use, index)]
SINKS = {
    "putstr": [("window", 2)],
    "add_menu": [("menu", 7)],
    "add_menu_str": [("menu", 1)],
    "add_menu_heading": [("menu", 1)],
    "end_menu": [("menu", 1)],
    "yn_function": [("query", 0)],
    "y_n": [("query", 0)],
    "ynq": [("query", 0)],
    "ynaq": [("query", 0)],
    "nyaq": [("query", 0)],
    "nyNaq": [("query", 0)],
    "getlin": [("query", 0)],
    "paranoid_query": [("query", 1)],
    "paranoid_ynq": [("query", 1)],
    "getdir": [("query", 0)],
    "query_objlist": [("menu", 0)],
    "query_category": [("menu", 0)],
    "getobj": [("getobj", 0)],
    "ggetobj": [("getobj", 0)],
    "set_occupation": [("occupation", 1)],
    "unmul": [("pline", 0)],
    "enlght_out": [("window", 0)],
    "losehp": [("death", 1)],
    "losexp": [("death", 0)],
    "instapetrify": [("death", 0)],
    "poisoned": [("pline", 0), ("death", 2)],
    "make_sick": [("death", 1)],
    "make_slimed": [("pline", 1)],
    "make_stoned": [("pline", 1), ("death", 3)],
}
# Assignments shown later: gn.nomovemsg when the hero can move again,
# gm.multi_reason in the cause of death ("while frozen by a potion")
ASSIGN_SINKS = {"nomovemsg": "pline", "multi_reason": "death"}
# Buffers: name -> (index of the destination, index of the text, printf?)
BUFFER_OPS = {
    "Sprintf": (0, 1, True),
    "sprintf": (0, 1, True),
    "Snprintf": (0, 2, True),
    "snprintf": (0, 2, True),
    "Sprintf1": (0, 1, False),
    "Strcpy": (0, 1, False),
    "strcpy": (0, 1, False),
    "copynchars": (0, 1, False),
    "Strcat": (0, 1, False),
    "strcat": (0, 1, False),
}
APPENDS = {"Strcat", "strcat"}
# calls that read a buffer without showing it
BUFFER_QUERIES = {"strlen", "strcmp", "strncmp", "strcmpi", "strncmpi", "strstri",
                  "strstr", "strchr", "strrchr", "index", "rindex", "eos", "c_eos",
                  "sizeof", "lowc", "highc", "digit", "letter", "isspace", "strsubst",
                  "strNsubst", "mungspaces", "trimspaces", "upstart", "lcase", "ucase",
                  "strip_newline", "Strlen", "BSTRNCMPI", "BSTRCMPI", "fuzzymatch"}
# Not shown to a player in a normal game: never in the catalog.
NOT_SHOWN = {"impossible", "panic", "debugpline0", "debugpline1", "debugpline2",
             "debugpline3", "debugpline4", "config_error_add", "paniclog",
             "nhassert_failed", "ifdebugresist", "warning", "error", "printf",
             "fprintf", "nh_terminate", "raw_print", "Fprintf", "dlb_fopen",
             "fopen", "nhl_error", "luaL_error", "strcmp", "strcmpi", "strncmpi",
             "strncmp", "strstri", "strstr", "strchr", "strrchr", "sscanf",
             "index", "rindex", "fuzzymatch", "match_optname", "getenv",
             "sanitize_name", "lua_getfield", "lua_setfield", "lua_pushstring",
             "nhl_add_table_entry_str", "nhl_add_table_entry_int",
             "nhl_add_table_entry_bool", "nhl_add_table_entry_char",
             "dump_plines", "putmsghistory"}
# Texts laid out as a picture or a table rather than said: the tombstone,
# #overview, the vanquished and genocided lists. Their entries are used as
# "layout": the client draws them from data, and the coverage of messages
# leaves them out.
LAYOUT_FUNCS = {"genl_outrip", "center", "list_vanquished", "list_genocided",
                "print_mapseen", "print_branch", "show_overview",
                "traverse_mapseenchn", "dooverview", "overview_stats"}
# The tombstone's lines with text centered between its sides
# ("                  |       Hero       |"): a pattern per side.
RIP_SIDE = re.compile(r"^( *\*?)\|( +)\|( *\*?)$")
# The enlightenment lines of ^X: enl_msg(prefix, present, past, suffix, ps)
# shows " <prefix><present or past><suffix><ps>."
ENL = {
    "enl_msg": None,
    "you_are": ("You ", "are ", "were "),
    "you_have": ("You ", "have ", "had "),
    "you_can": ("You ", "can ", "could "),
    "you_have_been": ("You ", "have been ", "were "),
    "you_have_never": ("You ", "have never ", "never "),
    "you_have_X": ("You ", "have ", ""),
}
ENL_CONTRACTIONS = ((" are not ", " aren't "), (" were not ", " weren't "),
                    (" have not ", " haven't "), (" had not ", " hadn't "),
                    (" can not ", " can't "), (" could not ", " couldn't "))

# ------------------------------------------------- argument kinds

KIND_FUNCS = {}
for _k, _names in {
    "monster": """x_monnam l_monnam mon_nam noit_mon_nam some_mon_nam Monnam
        noit_Monnam Some_Monnam noname_monnam m_monnam y_monnam YMonnam
        Adjmonnam a_monnam Amonnam distant_monnam mon_nam_too minimal_monnam
        priestname shkname Shknam ghostname""",
    "species": """pmname mon_pmname obj_pmname rndmonnam bogusmon monexplain
        rndghostname roguename coyotename rndorcname""",
    "object": """xname xname_flags minimal_xname mshot_xname doname doname_base
        doname_with_price doname_vague_quan corpse_xname cxname
        cxname_singular killer_xname short_oname singular Doname2 paydoname
        yname Yname2 ysimple_name Ysimple_name2 simpleonames ansimpleoname
        thesimpleoname bare_artifactname distant_name artiname artifact_name
        simple_typename obj_typename OBJ_NAME OBJ_DESCR tin_details
        safe_typename dump_typename bottlename""",
    "word": """body_part mbodypart hcolor rndcolor hliquid surface ceiling
        locomotion stagger on_fire msummon_environ u_locomotion mswings_verb
        mpoisons_subj trapname a_gname a_gname_at u_gname align_gname
        halu_gname align_gtitle align_str rank_of rank fingers_or_gloves
        Hello Goodbye dfeature_at role_gender_name explain_terrain""",
    "number": "sitoa itoa",
}.items():
    for _n in _names.split():
        KIND_FUNCS[_n] = _k
# x(name) keeps name's kind: an(xname(obj)) is an object
WRAPPERS = set("""an An the The upstart upwords lcase ucase s_suffix makeplural
    makesingular ing_suffix strip_the_prefix just_an capitalize highc
    trimspaces mungspaces""".split())
# Helpers whose text in a buffer argument is read from their definition
# (insight.c's ^X lines): the call writes what the helper leaves there
WRITTEN_BY = {"trap_predicament"}
# ... or appends it to what the buffer held ("For you, " + "esteemed sir")
APPENDED_BY = {"append_honorific"}
# Helpers that return a static buffer: what that buffer holds
RETURNED_BUFFER = {"piousness"}
# x(obj, "verb") is "<the object's name> verb[s]"
OBJVERB = {"aobjnam", "yobjnam", "Yobjnam2", "Tobjnam"}
# calls and names whose value is one of a few literals
LITERAL_CALLS = {
    "uhe": ["he", "she"], "uhim": ["him", "her"], "uhis": ["his", "her"],
    "mhe": ["he", "she", "it", "they"], "mhim": ["him", "her", "it", "them"],
    "mhis": ["his", "her", "its", "their"],
    "noit_mhe": ["he", "she", "it", "they"], "noit_mhim": ["him", "her", "it", "them"],
    "noit_mhis": ["his", "her", "its", "their"],
    "plur": ["", "s"], "currency": ["zorkmid", "zorkmids"],
    "ordin": ["st", "nd", "rd", "th"],
}
NULL_POINTERS = {"0", "NULL", "(char *) 0", "(const char *) 0", "(char*) 0",
                 "(const char*) 0", "(genericptr_t) 0", "nul", "emptystr"}
CONV_RE = re.compile(
    r"%(?P<flags>[-+ #0]*)(?P<width>\*|\d+)?(?:\.(?P<prec>\*|\d*))?"
    r"(?P<len>hh|h|ll|l|L|z|j|t|I64)?(?P<conv>[diouxXeEfgGcspn%])"
)


def conversions(fmt):
    """The conversions of a printf format (not %%)."""
    return [m for m in CONV_RE.finditer(fmt) if m.group("conv") != "%"]


def conv_kind(m):
    c = m.group("conv")
    if c in "diouxXeEfgG":
        return "number"
    if c == "c":
        return "char"
    if c == "p":
        return "pointer"
    return "text"


def escape(s):
    return s.replace("%", "%%")


def verb_s(verb):
    """vtense(singular subject, verb): the third person singular."""
    low = verb.lower()
    if low == "are":
        return verb[:-3] + "is"
    if low == "have":
        return verb[:-2] + "s"
    if (low[-1:] in ("z", "x", "s") or (len(low) >= 2 and low[-1] == "h" and low[-2] in "cs")
            or (len(low) == 2 and low[-1] == "o")):
        return verb + "es"
    if low[-1:] == "y" and len(low) >= 2 and low[-2] not in "aeiou":
        return verb[:-1] + "ies"
    return verb + "s"


def strip_expr(toks):
    """Drop enclosing parentheses and leading casts."""
    while toks:
        if toks[0].text == "(" and match_close(toks, 0) == len(toks) - 1:
            toks = toks[1:-1]
            continue
        if toks[0].text == "(":
            close = match_close(toks, 0)
            inner = toks[1:close]
            if (close < len(toks) - 1 and inner
                    and all(t.kind == "ident" or t.text == "*" for t in inner)
                    and (inner[-1].text == "*" or inner[0].text in ("const", "char", "int", "long",
                                                                    "unsigned", "void", "boolean"))):
                toks = toks[close + 1:]
                continue
        break
    return toks


def find_top(toks, text):
    depth = 0
    for i, t in enumerate(toks):
        if t.kind == "punct":
            if t.text in "([{":
                depth += 1
            elif t.text in ")]}":
                depth -= 1
            elif t.text == text and depth == 0:
                return i
    return None


def ternary(toks):
    """(cond, a, b) of `cond ? a : b`, else None."""
    q = find_top(toks, "?")
    if q is None:
        return None
    depth = 0
    nest = 0
    for i in range(q + 1, len(toks)):
        t = toks[i]
        if t.kind != "punct":
            continue
        if t.text in "([{":
            depth += 1
        elif t.text in ")]}":
            depth -= 1
        elif depth == 0 and t.text == "?":
            nest += 1
        elif depth == 0 and t.text == ":":
            if nest == 0:
                return toks[:q], toks[q + 1:i], toks[i + 1:]
            nest -= 1
    return None


def literal_values(toks):
    """The literals an expression of literals and conditions can be
    (`a ? "x" : b ? "y" : "z"`), or None."""
    toks = strip_expr(toks)
    if not toks:
        return None
    tern = ternary(toks)
    if tern:
        a, b = literal_values(tern[1]), literal_values(tern[2])
        return None if a is None or b is None else dedupe(a + b)
    if all(t.kind == "string" for t in toks):
        return ["".join(t.value for t in toks)]
    return None


def call_parts(toks):
    """(name, args) when the expression is one call `name(args)`."""
    if (len(toks) >= 3 and toks[0].kind == "ident" and toks[1].text == "("
            and match_close(toks, 1) == len(toks) - 1):
        return toks[0].text, split_args(toks, 1, len(toks) - 1)
    return None


def dedupe(xs):
    seen = set()
    out = []
    for x in xs:
        key = x if isinstance(x, str) else (x[0], tuple(x[1]))
        if key not in seen:
            seen.add(key)
            out.append(x)
    return out


def destination(toks):
    """(buffer name, append?) of a buffer operation's destination: buf,
    eos(buf), buf + n, &buf[n]."""
    toks = strip_expr(toks)
    call = call_parts(toks)
    if call and call[0] in ("eos", "c_eos") and call[1]:
        name, _ = destination(call[1][0])
        return name, True
    append = False
    if toks and toks[0].text == "&":
        toks = toks[1:]
        append = True
    if find_top(toks, "+") is not None:
        append = True
        toks = toks[:find_top(toks, "+")]
    # the whole lvalue: buf, svk.killer.name, mtmp->mgivenname
    if toks and toks[0].kind == "ident":
        name = []
        for t in toks:
            if t.kind == "ident" or t.text in (".", "->"):
                name.append(t.text)
            else:
                break
        return "".join(name), append
    return None, False


class Op:
    """A write to a local buffer; `transform` is a change in place
    (mungspaces, the first letter's case) instead of a text written."""

    def __init__(self, index, append, text, args, printf, line, call, transform=None):
        self.index = index
        self.append = append
        self.text = text
        self.args = args
        self.printf = printf
        self.line = line
        self.call = call
        self.transform = transform
        # the change may not happen (strsubst that finds nothing, or in a
        # branch): the text as it was, too
        self.either = False
        # safe_qbuf: (prefix, suffix) around an object's name
        self.around = None

    def changed(self, built):
        """A buffer's texts after this change in place."""
        out = [(self.transform(f), kinds) for f, kinds in built]
        return dedupe(built + out) if self.either else out


def squeeze(fmt):
    return re.sub(r" {2,}", " ", fmt).strip(" ")


def lower_first(fmt):
    return fmt[:1].lower() + fmt[1:] if fmt[:1].isalpha() else fmt


def upper_first(fmt):
    return fmt[:1].upper() + fmt[1:] if fmt[:1].isalpha() else fmt


# in-place changes of a buffer: name(buf); as a statement
TRANSFORMS = {"mungspaces": squeeze, "trimspaces": squeeze, "upstart": upper_first}


def lower_all(fmt):
    """lcase(): every letter small but the conversions'."""
    out, last = [], 0
    for m in CONV_RE.finditer(fmt):
        out += [fmt[last:m.start()].lower(), m.group(0)]
        last = m.end()
    return "".join(out) + fmt[last:].lower()


# the case a call's text gets from a call around it: lcase(f(…, buf))
CASE_WRAPPERS = {"lcase": lower_all, "upstart": upper_first}


class Globals:
    """What every file sees: string macros of the headers, the common
    strings (nothing_happens...), the tables of words; every function's
    context and call sites, for what a parameter or a return value can
    be."""

    def __init__(self):
        self.strings = {}
        self.arrays = {}
        self.structs = {}
        self.tables = {}
        # function name -> for each parameter: may it write it (char *)?
        self.prototypes = {}
        self.defs = {}  # function name -> [Context]
        self.callers = {}  # function name -> [(Context, args)]
        # function name -> [(Context, args, token index of the call)]
        self.call_sites_at = {}
        self.memo = {}

    def index(self, ctx):
        self.defs.setdefault(ctx.func.name, []).append(ctx)
        for name, args, pos in ctx.calls:
            self.callers.setdefault(name, []).append((ctx, args))
            self.call_sites_at.setdefault(name, []).append((ctx, args, pos))

    def param_pieces(self, ctx, name, depth):
        """What callers pass for a parameter that is no literal: the texts
        of the buffers they hand over ("Call %s:" to name_from_player)."""
        def compute():
            k = ctx.param_list.index(name)
            sites = self.call_sites_at.get(ctx.func.name, [])
            if len(self.defs.get(ctx.func.name, [])) > 1:
                sites = [x for x in sites if x[0].unit is ctx.unit]
            out = []
            for caller, args, pos in sites:
                if k >= len(args) or caller.values(args[k]) is not None:
                    continue
                t = strip_expr(args[k])
                if len(t) == 1 and t[0].kind == "ident" and t[0].text in caller.ops:
                    out += caller.compositions(t[0].text, pos, depth + 1)
            # (every caller's texts: as many as a buffer holds)
            return dedupe(out)[:MAX_HELD]
        return self._memo(("ppc", ctx.unit.path, ctx.func.name, name, depth), compute, [])

    def _memo(self, key, compute, unknown=None):
        if key in self.memo:
            return self.memo[key]
        self.memo[key] = unknown  # what a cycle answers
        value = compute()
        self.memo[key] = value
        return value

    def definitions(self, name, unit):
        defs = self.defs.get(name, [])
        if len(defs) > 1:
            defs = [d for d in defs if d.unit is unit] or defs[:1]
        return defs

    def call_sites(self, ctx):
        sites = self.callers.get(ctx.func.name, [])
        if len(self.defs.get(ctx.func.name, [])) > 1:
            sites = [s for s in sites if s[0].unit is ctx.unit]
        return sites

    def param_values(self, ctx, name):
        """The literals every caller passes for a parameter, or None."""
        def compute():
            k = ctx.param_list.index(name)
            sites = self.call_sites(ctx)
            if not sites:
                return None
            out = []
            for caller, args in sites:
                if k >= len(args):
                    return None
                if source_text(strip_expr(args[k])) in NULL_POINTERS or source_text(args[k]) in NULL_POINTERS:
                    continue
                v = caller.values(args[k])
                if v is None:
                    return None
                out += v
            return dedupe(out) if out else None
        return self._memo(("pv", ctx.unit.path, ctx.func.name, name), compute)

    def writes(self, fname, k):
        """May function `fname` write its argument k (a `char *`
        parameter, by its definition or extern.h)?"""
        for d in self.defs.get(fname, []):
            if k < len(d.param_writable):
                return d.param_writable[k]
        proto = self.prototypes.get(fname)
        return bool(proto and k < len(proto) and proto[k])

    def param_writes(self, fname, k, unit, depth):
        """What function `fname` leaves in its argument k, a buffer it
        writes: what that buffer holds at the end of its body
        (trap_predicament: "trapped in %s", "stuck in %s"...), or None."""
        def compute():
            out = []
            for d in self.definitions(fname, unit):
                if k >= len(d.param_list) or d.param_list[k] not in d.ops:
                    return None
                held = d.compositions(d.param_list[k], d.func.body[1], depth + 1)
                if not held:
                    return None
                out += held
            return dedupe(out)[:MAX_HELD] if out else None
        return self._memo(("pw", unit.path, fname, k), compute)

    def returned_buffer(self, fname, unit, depth):
        """What a function of RETURNED_BUFFER returns: what the buffer it
        returns holds there (piousness: "piously aligned"...), or None."""
        def compute():
            out = []
            for d in self.definitions(fname, unit):
                for expr in d.returns:
                    t = strip_expr(expr)
                    if len(t) != 1 or t[0].text not in d.ops:
                        return None
                    out += d.compositions(t[0].text, d.func.body[1], depth + 1)
            return dedupe(out)[:MAX_HELD] if out else None
        return self._memo(("rb", unit.path, fname), compute)

    def member_values(self, unit, member):
        """The literals a struct member is set to in `unit`, when it is set
        to nothing else (doengrave_ctx_verb's `de->everb = de->adding ?
        "add to the writing in" : "write in"`), or None."""
        def compute():
            toks = unit.toks
            out = []
            for i, t in enumerate(toks):
                if not (t.kind == "ident" and t.text == member and 0 < i < len(toks) - 1
                        and toks[i - 1].text in ("->", ".") and toks[i + 1].text == "="):
                    continue
                j = i + 2
                while j < len(toks) and toks[j].text != ";":
                    j = match_close(toks, j) + 1 if toks[j].text in "([{" else j + 1
                v = literal_values(toks[i + 2:j])
                if v is None:
                    return None
                out += v
            return dedupe(out) or None
        return self._memo(("mv", unit.path, member), compute)

    def param_values_partial(self, ctx, name):
        """The literals the callers that pass literals give a parameter."""
        def compute():
            k = ctx.param_list.index(name)
            out = []
            for caller, args in self.call_sites(ctx):
                if k < len(args):
                    v = caller.values(args[k])
                    if v is not None:
                        out += v
            return dedupe(out)
        return self._memo(("pp", ctx.unit.path, ctx.func.name, name), compute, [])

    def param_kind(self, ctx, name):
        def compute():
            k = ctx.param_list.index(name)
            kinds = set()
            for caller, args in self.call_sites(ctx):
                if k < len(args) and caller.values(args[k]) is None:
                    kinds.add(caller.kind(args[k]))
            kinds.discard("text")
            return kinds.pop() if len(kinds) == 1 else "text"
        return self._memo(("pk", ctx.unit.path, ctx.func.name, name), compute, "text")

    def return_values(self, name, unit):
        """The literals a function returns, or None."""
        def compute():
            defs = self.definitions(name, unit)
            if not defs:
                return None
            out = []
            for d in defs:
                if not d.returns:
                    return None
                for expr in d.returns:
                    if source_text(strip_expr(expr)) in NULL_POINTERS:
                        continue
                    v = d.values(expr)
                    if v is None:
                        return None
                    out += v
            return dedupe(out) if out else None
        return self._memo(("rv", unit.path, name), compute)

    def return_kind(self, name, unit):
        def compute():
            kinds = set()
            for d in self.definitions(name, unit):
                for expr in d.returns:
                    if source_text(strip_expr(expr)) not in NULL_POINTERS:
                        kinds.add(d.kind(expr))
            kinds.discard("text")
            return kinds.pop() if len(kinds) == 1 else "text"
        return self._memo(("rk", unit.path, name), compute, "text")


class Context:
    """One function: its parameters, the literal assignments of its
    variables, the buffers it writes."""

    def __init__(self, glob, unit, func):
        self.glob = glob
        self.unit = unit
        self.func = func
        toks = unit.toks
        self.param_list = self._params()
        self.params = set(self.param_list)
        self.assigns = {}
        self.arrays = {}
        self.ops = {}
        self.returns = []
        self.calls = []
        # where each variable is a whole argument of a call: a buffer is
        # shown (or handed on) there; a call that may write it clobbers it
        self.uses = {}
        self.passes = {}
        # a call wrapped in lcase()/upstart(), by its token index
        self.cased = {}
        # the texts of an argument a format had too many of to derive
        self.left_out = []
        self._depths = None
        start, end = func.body
        i = start + 1
        while i < end:
            t = toks[i]
            if t.text == "return" and toks[i + 1].text != ";":
                j = expression_end(toks, i + 1, end)
                self.returns.append(toks[i + 1:j])
            if (t.kind == "ident" and toks[i + 1].text == "(" and toks[i - 1].text not in (".", "->")
                    and t.text not in BUFFER_OPS):
                close = match_close(toks, i + 1)
                args = split_args(toks, i + 1, close)
                self.calls.append((t.text, args, i))
                if t.text not in BUFFER_QUERIES:
                    # a call that shows the buffer reads it; a function whose
                    # parameter is a `char *` may write it
                    # (fmt_elapsed_time(buf, final)): resolved later.
                    # safe_qbuf writes its first argument (an op), reading it
                    # first as its prefix: no use of that buffer there
                    written = destination(args[0])[0] if t.text == "safe_qbuf" and args else None
                    for k, a in enumerate(args):
                        a = strip_expr(a)
                        if len(a) == 1 and a[0].kind == "ident" and a[0].text != written:
                            self.uses.setdefault(a[0].text, []).append(i)
                            self.passes.setdefault(a[0].text, {})[i] = (t.text, k)
                    if toks[i - 1].text == "(" and toks[i - 2].text in CASE_WRAPPERS:
                        # lcase(skill_level_name(w, buf)): the text it writes, so
                        self.cased[i] = CASE_WRAPPERS[toks[i - 2].text]
            if t.text == "=" and toks[i - 1].kind == "ident" and toks[i - 2].text not in (".", "->"):
                name = toks[i - 1].text
                if toks[i + 1].text == "{":
                    close = match_close(toks, i + 1)
                    values = clex.string_array(toks[i + 2:close])
                    if values is not None:
                        self.arrays[name] = values
                    i = close + 1
                    continue
                j = expression_end(toks, i + 1, end)
                self.assigns.setdefault(name, []).append(toks[i + 1:j])
                # the calls in the value are scanned too
                i += 1
                continue
            if t.text == "=" and toks[i - 1].text == "]" and toks[i + 1].kind == "string":
                # static const char name[] = "...": a local string
                k = i - 1
                while k > start and toks[k].text != "[":
                    k -= 1
                if toks[k - 1].kind == "ident":
                    j = expression_end(toks, i + 1, end)
                    self.assigns.setdefault(toks[k - 1].text, []).append(toks[i + 1:j])
            if t.text == "=" and toks[i - 1].text == "]" and toks[i + 1].text == "{":
                # name[...] = {...}: a local table
                k = i - 1
                while k > start and toks[k].text != "[":
                    k -= 1
                close = match_close(toks, i + 1)
                if toks[k - 1].kind == "ident":
                    values = clex.string_array(toks[i + 2:close])
                    if values is not None:
                        self.arrays[toks[k - 1].text] = values
                i = close + 1
                continue
            if (t.kind == "ident" and t.text in TRANSFORMS and toks[i + 1].text == "("
                    and toks[i + 2].kind == "ident" and toks[i + 3].text == ")"
                    and toks[i - 1].text in (";", "{", "}", ")") and toks[i + 4].text == ";"):
                # mungspaces(buf); (or after a (void) cast)
                self.ops.setdefault(toks[i + 2].text, []).append(
                    Op(i, True, [], [], False, t.line, t.text, TRANSFORMS[t.text]))
            if (t.kind == "ident" and t.text == "strsubst" and toks[i + 1].text == "("
                    and toks[i + 2].kind == "ident" and toks[i + 3].text == ","
                    and toks[i + 4].kind == "string" and toks[i + 5].text == ","
                    and toks[i + 6].kind == "string" and toks[i + 7].text == ")"
                    and toks[i - 1].text in (";", "{", "}", ")") and toks[i + 8].text == ";"):
                # (void) strsubst(buf, "limbs", "extremities"): the first
                # of the one is the other
                was, now = escape(toks[i + 4].value), escape(toks[i + 6].value)
                op = Op(i, True, [], [], False, t.line, t.text,
                        lambda f, was=was, now=now: f.replace(was, now, 1))
                op.either = True
                self.ops.setdefault(toks[i + 2].text, []).append(op)
            if (t.text == "*" and toks[i + 1].kind == "ident" and toks[i + 2].text == "="
                    and toks[i + 3].text in ("lowc", "highc") and toks[i + 4].text == "("
                    and toks[i + 5].text == "*" and toks[i + 6].text == toks[i + 1].text):
                # *buf = lowc(*buf): the first letter's case
                fn = lower_first if toks[i + 3].text == "lowc" else upper_first
                self.ops.setdefault(toks[i + 1].text, []).append(
                    Op(i, True, [], [], False, t.line, toks[i + 3].text, fn))
            if (t.kind == "ident" and toks[i + 1].text == "[" and toks[i + 2].text == "0"
                    and toks[i + 3].text == "]" and toks[i + 4].text == "="
                    and toks[i + 5].text in ("'\\0'", "0") and toks[i - 1].text not in (".", "->")):
                # buf[0] = '\0': the buffer is emptied
                self.ops.setdefault(t.text, []).append(
                    Op(i, False, [clex.Tok("string", '""', t.line, "")], [], False, t.line, "="))
            if (t.text == "*" and toks[i + 1].kind == "ident" and toks[i + 2].text == "="
                    and toks[i + 3].text in ("'\\0'", "0") and toks[i - 1].text in (";", "{", "}", ")")):
                self.ops.setdefault(toks[i + 1].text, []).append(
                    Op(i, False, [clex.Tok("string", '""', t.line, "")], [], False, t.line, "="))
            if (t.kind == "ident" and t.text == "safe_qbuf" and toks[i + 1].text == "("
                    and toks[i - 1].text not in (".", "->")):
                # safe_qbuf(buf, prefix, suffix, obj, ...): prefix, the
                # object's name, suffix
                close = match_close(toks, i + 1)
                args = split_args(toks, i + 1, close)
                dest, _ = destination(args[0]) if args else (None, False)
                if dest and len(args) >= 3:
                    op = Op(i, False, [], [], False, t.line, t.text)
                    op.around = (args[1], args[2])
                    self.ops.setdefault(dest, []).append(op)
            if (t.kind == "ident" and t.text in BUFFER_OPS and toks[i + 1].text == "("
                    and toks[i - 1].text not in (".", "->")):
                close = match_close(toks, i + 1)
                args = split_args(toks, i + 1, close)
                didx, tidx, printf = BUFFER_OPS[t.text]
                if len(args) > tidx:
                    dest, append = destination(args[didx])
                    if dest:
                        op = Op(i, append or t.text in APPENDS, args[tidx], args[tidx + 1:],
                                printf, t.line, t.text)
                        self.ops.setdefault(dest, []).append(op)
            i += 1

    def _params(self):
        toks = self.unit.toks
        k = self.func.body[0] - 1
        while k > 0 and toks[k].text != ")":
            k -= 1
        open_i = k
        depth = 0
        while open_i > 0:
            if toks[open_i].text == ")":
                depth += 1
            elif toks[open_i].text == "(":
                depth -= 1
                if depth == 0:
                    break
            open_i -= 1
        params = []
        self.param_writable = []
        for arg in split_args(toks, open_i, k):
            self.param_writable.append(writable(arg))
            # int (*name)(OBJ_P): the name is in the first parentheses
            fn = [j for j in range(len(arg) - 2) if arg[j].text == "(" and arg[j + 1].text == "*"]
            if fn and arg[fn[0] + 2].kind == "ident":
                params.append(arg[fn[0] + 2].text)
                continue
            names = [t.text for t in arg if t.kind == "ident"]
            params.append(names[-1] if names else "")
        return params

    def string_const(self, name):
        if name in self.unit.strings:
            return self.unit.strings[name]
        return self.glob.strings.get(name)

    def array(self, name):
        for table in (self.arrays, self.unit.arrays, self.glob.arrays):
            if name in table:
                return table[name]
        return None

    def values(self, toks, depth=0, seen=()):
        """The literals an expression can be, or None when it can be
        something else."""
        if depth > 8:
            return None
        toks = strip_expr(toks)
        if not toks:
            return None
        tern = ternary(toks)
        if tern:
            a = self.values(tern[1], depth + 1, seen)
            b = self.values(tern[2], depth + 1, seen)
            if a is not None and b is not None:
                return dedupe(a + b)
            return None
        if all(t.kind == "string" or (t.kind == "ident" and self.string_const(t.text) is not None)
               for t in toks):
            return ["".join(t.value if t.kind == "string" else self.string_const(t.text) for t in toks)]
        call = call_parts(toks)
        if call:
            name, args = call
            if name in LITERAL_CALLS:
                return LITERAL_CALLS[name]
            if name in ("vtense", "otense") and len(args) == 2:
                verbs = self.values(args[1], depth + 1, seen)
                if verbs is not None:
                    return dedupe([f for v in verbs for f in (v, verb_s(v))])
            if name == "ROLL_FROM" and len(args) == 1:
                # ROLL_FROM(h_sounds): one of the array's strings
                arg = strip_expr(args[0])
                if len(arg) == 1 and arg[0].kind == "ident":
                    return self.array(arg[0].text)
            if name in ("upstart", "capitalize", "highc") and len(args) == 1:
                inner = self.values(args[0], depth + 1, seen)
                if inner is not None:
                    return [v[:1].upper() + v[1:] for v in inner]
            if name in WRAPPERS or name in OBJVERB:
                return None
            returned = self.glob.return_values(name, self.unit)
            if name in KIND_FUNCS and returned is not None and len(returned) > 4:
                # a word of a large set (a body part, a colour): the
                # lexicon's, not one entry per word
                return None
            return returned
        # x->member: what its unit sets it to
        if (len(toks) == 3 and toks[0].kind == "ident" and toks[1].text in ("->", ".")
                and toks[2].kind == "ident"):
            return self.glob.member_values(self.unit, toks[2].text)
        if len(toks) == 1 and toks[0].kind == "ident":
            name = toks[0].text
            if name in LITERAL_CALLS:
                return LITERAL_CALLS[name]
            if name in self.ops or name in seen:
                return None
            out = []
            if name in self.params:
                v = self.glob.param_values(self, name)
                if v is None:
                    return None
                out += v
            elif name not in self.assigns:
                return None
            for rhs in self.assigns.get(name, []):
                if source_text(rhs) in NULL_POINTERS:
                    continue
                v = self.values(rhs, depth + 1, seen + (name,))
                if v is None:
                    return None
                out += v
            return dedupe(out) if out else None
        # table[i]
        if (toks[0].kind == "ident" and len(toks) >= 4 and toks[1].text == "["
                and match_close(toks, 1) == len(toks) - 1):
            table = self.array(toks[0].text)
            if table:
                return dedupe(table)
        # rows[i].member: the member of every row of a struct table
        if (toks[0].kind == "ident" and len(toks) >= 6 and toks[1].text == "["
                and match_close(toks, 1) == len(toks) - 3 and toks[-2].text == "."
                and toks[-1].kind == "ident"):
            column = self.column(toks[0].text, toks[-1].text)
            if column:
                return column
        # rows[i][k]: column k of a table of rows of strings
        if (toks[0].kind == "ident" and len(toks) >= 7 and toks[1].text == "["
                and toks[-1].text == "]" and toks[-3].text == "[" and toks[-2].kind == "number"
                and toks[-2].text.isdigit() and match_close(toks, 1) == len(toks) - 4):
            column = self.column(toks[0].text, int(toks[-2].text))
            if column:
                return column
        return None

    def column(self, table, member):
        """The literals of `member` in every row of struct table `table`;
        None when a row holds something else."""
        found = self.unit.tables.get(table) or self.glob.tables.get(table)
        if not found:
            return None
        tag, rows = found
        if isinstance(member, int):
            k = member
        else:
            members = (self.unit.structs.get(tag) or self.glob.structs.get(tag)) if tag else None
            if not members or member not in members:
                return None
            k = members.index(member)
        out = []
        for row in rows:
            if k >= len(row):
                continue
            if source_text(strip_expr(row[k])) in NULL_POINTERS:
                continue
            v = self.values(row[k])
            if v is None:
                return None
            out += v
        return dedupe(out) or None

    def kind(self, toks, depth=0, seen=()):
        """What a %s argument names, when it is not a literal."""
        if depth > 8:
            return "text"
        toks = strip_expr(toks)
        if not toks:
            return "text"
        tern = ternary(toks)
        if tern:
            kinds = set()
            for side in tern[1:]:
                if self.values(side) is None:
                    kinds.add(self.kind(side, depth + 1, seen))
            return "|".join(sorted(kinds)) if kinds else "text"
        call = call_parts(toks)
        if call:
            name, args = call
            if name in KIND_FUNCS:
                return KIND_FUNCS[name]
            if name in OBJVERB:
                return "object"
            if name in WRAPPERS and args:
                return self.kind(args[0], depth + 1, seen)
            if re.search(r"mon_?nam$|Monnam$", name):
                return "monster"
            return self.glob.return_kind(name, self.unit)
        if len(toks) == 1 and toks[0].kind == "ident":
            name = toks[0].text
            if name in seen:
                return "text"
            kinds = set()
            if name in self.params:
                kinds.add(self.glob.param_kind(self, name))
            for rhs in self.assigns.get(name, []):
                if source_text(rhs) not in NULL_POINTERS:
                    kinds.add(self.kind(rhs, depth + 1, seen + (name,)))
            kinds.discard("text")
            if len(kinds) == 1:
                return kinds.pop()
            return "text"
        text = source_text(toks)
        if re.search(r"\b(plname|pl_fruit)\b", text):
            # typed by the player: shown as typed
            return "player"
        if re.search(r"pmnames\s*\[|^mons\[", text):
            return "species"
        if re.search(r"oc_name|oc_descr|OBJ_NAME|OBJ_DESCR", text):
            return "object"
        if re.search(r"\.explanation|defsyms", text):
            return "word"
        return "text"

    # ---- the formats a call can show

    def alternatives(self, m, arg, pos, depth):
        """What conversion m with argument tokens `arg` can show: pieces
        (format text, kinds of its conversions)."""
        plain = m.group("conv") == "s" and m.group("width") is None and m.group("prec") is None
        if not plain:
            return [(m.group(0), [conv_kind(m)])]
        return self.pieces(arg, pos, depth)

    def pieces(self, toks, pos, depth, seen=()):
        """What an expression shown as text can be: its literals, the verb
        forms of Tobjnam(obj, "verb"), what a local buffer holds, the
        values of a variable's assignments; a placeholder for the rest."""
        v = self.values(toks)
        if v is not None:
            return [(escape(x), []) for x in v]
        placeholder = [("%s", [self.kind(toks)])]
        if depth >= MAX_DEPTH:
            return placeholder
        t = strip_expr(toks)
        tern = ternary(t)
        if tern:
            return dedupe(self.pieces(tern[1], pos, depth + 1, seen)
                          + self.pieces(tern[2], pos, depth + 1, seen))[:MAX_DERIVED]
        call = call_parts(t)
        if call and call[0] in RETURNED_BUFFER:
            held = self.glob.returned_buffer(call[0], self.unit, depth)
            if held:
                return held
        if call and call[0] in OBJVERB and len(call[1]) == 2:
            verbs = self.values(call[1][1])
            if verbs is not None:
                forms = dedupe([f for x in verbs for f in (x, verb_s(x))])
                return [("%s " + escape(f), ["object"]) for f in forms]
        if len(t) != 1 or t[0].kind != "ident" or t[0].text in seen:
            return placeholder
        name = t[0].text
        if name in self.ops:
            return self.compositions(name, pos, depth + 1) or placeholder
        # a buffer only a helper writes: trap_predicament(predicament, …),
        # by the last call before `pos` that may write it
        writes = [(u, p) for u, p in self.passes.get(name, {}).items()
                  if u < pos and self.glob.writes(*p)]
        if writes:
            at, passed = max(writes)
            written = self.written(name, at, passed, depth + 1)
            if written:
                return written
        out = []
        if name in self.params:
            # the literals some callers pass, the buffers others hand over,
            # and the placeholder for the rest
            known = self.glob.param_values_partial(self, name)
            out += [(escape(x), []) for x in known]
            out += self.glob.param_pieces(self, name, depth)
            out += placeholder
        if name in self.assigns:
            for rhs in self.assigns[name]:
                if source_text(rhs) in NULL_POINTERS:
                    continue
                out += self.pieces(rhs, pos, depth + 1, seen + (name,))
        out = dedupe(out)
        return out[:MAX_HELD] if out else placeholder

    def variants(self, fmt, args, pos, depth=0):
        """Every text of format `fmt` with its arguments: literal arguments
        put in, buffers inlined. Pieces (format, kinds)."""
        convs = conversions(fmt)
        choices = []
        ai = 0
        for m in convs:
            ai += (m.group("width") == "*") + (m.group("prec") == "*")
            arg = args[ai] if ai < len(args) else []
            ai += 1
            choices.append(self.alternatives(m, arg, pos, depth))
        # too many combinations: the most varied arguments stay placeholders,
        # and the sentences among their texts pieces of their own
        # (godvoice's "Thou hast angered me.")
        while product_size(choices) > MAX_DERIVED:
            k = max(range(len(choices)), key=lambda c: len(choices[c]))
            self.left_out += [text for text, kinds in choices[k] if not kinds and " " in text.strip()]
            choices[k] = [(convs[k].group(0), [conv_kind(convs[k])])]
        out = []
        for combo in itertools.product(*choices):
            text = []
            kinds = []
            p = 0
            for m, (piece, piece_kinds) in zip(convs, combo):
                text.append(fmt[p:m.start()])
                text.append(piece)
                kinds += piece_kinds
                p = m.end()
            text.append(fmt[p:])
            out.append(("".join(text), kinds))
        return out

    def generic(self, fmt, args):
        """The format as it is, with the kinds of its conversions."""
        kinds = []
        ai = 0
        for m in conversions(fmt):
            ai += (m.group("width") == "*") + (m.group("prec") == "*")
            arg = args[ai] if ai < len(args) else []
            ai += 1
            kinds.append(self.kind(arg) if conv_kind(m) == "text" else conv_kind(m))
        return fmt, kinds

    def op_pieces(self, op, depth):
        """What one buffer operation writes."""
        if op.transform:
            return []
        if op.around:
            parts = [self.pieces(op.around[0], op.index, depth + 1), [("%s", ["object"])],
                     self.pieces(op.around[1], op.index, depth + 1)]
            while product_size(parts) > MAX_DERIVED:
                k = max((0, 2), key=lambda i: len(parts[i]))
                parts[k] = [("%s", ["text"])]
            return [("".join(p for p, _ in combo), [x for _, ks in combo for x in ks])
                    for combo in itertools.product(*parts)]
        if op.printf:
            fmts = self.values(op.text)
            if fmts is None:
                return []
            return [p for f in fmts for p in self.variants(f, op.args, op.index, depth)]
        return self.pieces(op.text, op.index, depth)

    def op_generic(self, op):
        """What one buffer operation writes, its arguments placeholders."""
        if op.printf:
            fmts = self.values(op.text)
            return [self.generic(f, op.args) for f in fmts] if fmts is not None else []
        return [("%s", [self.kind(op.text)])]

    def tail(self, op, ref, depth):
        """An append, as the text at token `ref` (a write or a use of the
        buffer) is followed by it."""
        return Tail(self.op_pieces(op, depth), self.op_generic(op), self.always_after(ref, op.index))

    def always_after(self, ref, i):
        """Does the statement at token i run whenever the code at token
        `ref` before it ran: not in a branch or a loop `ref` is not in?"""
        if self.guarded(i):
            return False
        d = self.depths()
        return min(d[k] for k in range(ref, i + 1)) >= d[i]

    def guarded(self, i):
        """Is the statement holding token i the body of an if, else, for,
        while, do or case without braces?"""
        toks = self.unit.toks
        j = i - 1
        while j > self.func.body[0]:
            t = toks[j].text
            if t in (";", "{", "}"):
                return False
            if t in ("else", "do", ":"):
                return True
            if t == ")":
                level = 0
                while j > 0:
                    if toks[j].text == ")":
                        level += 1
                    elif toks[j].text == "(":
                        level -= 1
                        if level == 0:
                            break
                    j -= 1
                if toks[j - 1].text in ("if", "for", "while", "switch"):
                    return True
            # an enclosing call's "(", a cast, an operand: further back
            j -= 1
        return False

    def depths(self):
        """The block depth at each token of the body."""
        if self._depths is None:
            toks = self.unit.toks
            start, end = self.func.body
            d, out = 0, {}
            for k in range(start, end + 1):
                out[k] = d
                if toks[k].text == "{":
                    d += 1
                elif toks[k].text == "}":
                    d -= 1
            self._depths = out
        return self._depths

    def compositions(self, name, pos, depth):
        """What buffer `name` can hold at token `pos`: what the operations
        since its last use before `pos` wrote (a buffer is reused for one
        text after another): each Sprintf/Strcpy followed by the Strcats
        after it that always run and by any of those in a branch, when
        these are few (more are a list, not a sentence). Which branch
        runs is not followed: an over-approximation."""
        uses = [u for u in self.uses.get(name, []) if u < pos]
        since = max(uses) if uses else -1
        ops = [op for op in self.ops.get(name, []) if since < op.index < pos]
        passed = self.passes.get(name, {}).get(since)
        if passed and passed[0] in APPENDED_BY and not any(not op.append for op in ops):
            # a helper appended to it ("For you, " + "esteemed sir"): one
            # text more after what it held there; a call in a branch may
            # not have run ("For you, scum;")
            before = self.compositions(name, since, depth)
            built = with_tails(before, [Tail([("%s", ["text"])], ("%s", ["text"]), True)])
            appends = [a for a in ops if not a.transform]
            tails = [t for t in (self.tail(a, since, depth) for a in appends) if t.pieces]
            out = with_tails(built, tails)
            d = self.depths()
            if self.guarded(since) or d[since] > d.get(pos, 0):
                # the appends of the call's own block went with it
                outer = [t for t in (self.tail(a, since, depth) for a in appends
                                     if d[a.index] < d[since]) if t.pieces]
                out = dedupe(out + with_tails(before, outer))
            return out
        if passed and self.glob.writes(*passed) and not any(not op.append for op in ops):
            # written by a call: what it holds is not known (but for a few
            # helpers read from their definition), what is appended to it
            # is, and all there is to say ("%s (current; limit:%s)": any
            # few of the appends)
            tails = [t for t in (self.tail(a, since, depth) for a in ops if not a.transform) if t.pieces]
            written = self.written(name, since, passed, depth)
            if written is not None:
                return with_tails(written, tails)
            return with_tails([("%s", ["text"])], tails, few=True) if tails else []
        if not any(not op.append for op in ops) and since >= 0:
            # appended (or not) to what it held at its last use
            before = self.compositions(name, since, depth)
            tails = [t for t in (self.tail(a, since, depth) for a in ops if not a.transform) if t.pieces]
            built = with_tails(before, tails)
            for a in ops:
                if a.transform:
                    built = a.changed(built)
            return built
        out = []
        for k, op in enumerate(ops):
            if op.append:
                continue
            heads = self.op_pieces(op, depth)
            later = [a for a in ops[k + 1:] if a.append]
            tails = [t for t in (self.tail(a, op.index, depth) for a in later if not a.transform)
                     if t.pieces]
            built = with_tails(heads, tails)
            for a in later:
                if a.transform:
                    built = a.changed(built)
            out += built
        first = next((op for op in ops if not op.append), None)
        if (first and passed and self.glob.writes(*passed)
                and not self.always_after(since, first.index)):
            # a call wrote it, and the write after it is in a branch the
            # call is not in ("if (final) N_times(n, buf); else if (n > 1)
            # Sprintf(buf, ...);"): the call's text goes on in the others
            appends = [a for a in ops if a.append and a.index < first.index and not a.transform]
            tails = [t for t in (self.tail(a, since, depth) for a in appends) if t.pieces]
            written = self.written(name, since, passed, depth)
            out += (with_tails(written, tails) if written is not None
                    else with_tails([("%s", ["text"])], tails, few=True))
        return dedupe(out)[:MAX_HELD]

    def written(self, name, at, passed, depth):
        """What the call at token `at` (`passed`: name, argument index)
        leaves in buffer `name`, for the helpers of WRITTEN_BY; None for
        any other."""
        fname, k = passed
        if fname not in WRITTEN_BY or depth > MAX_DEPTH:
            return None
        held = self.glob.param_writes(fname, k, self.unit, depth)
        if held is None:
            return None
        cased = self.cased.get(at)
        return [(cased(f), kinds) for f, kinds in held] if cased else held

    def formats(self, toks, pos):
        """The formats a format argument can be: its literals, or what the
        buffer it names holds."""
        v = self.values(toks)
        if v is not None:
            return [(f, None) for f in v]
        toks = strip_expr(toks)
        if len(toks) == 1 and toks[0].kind == "ident" and toks[0].text in self.ops:
            return [(f, kinds) for f, kinds in self.compositions(toks[0].text, pos, 1)]
        # &buf[i], buf + i: what it holds from offset i ("Wait!  " left out)
        name, offsets = None, None
        if (len(toks) >= 5 and toks[0].text == "&" and toks[1].kind == "ident"
                and toks[2].text == "[" and match_close(toks, 2) == len(toks) - 1):
            name, offsets = toks[1].text, self.int_values(toks[3:-1])
        elif len(toks) >= 3 and toks[0].kind == "ident" and toks[1].text == "+":
            name, offsets = toks[0].text, self.int_values(toks[2:])
        if name in self.ops and offsets:
            out = []
            for f, kinds in self.compositions(name, pos, 1):
                for k in offsets:
                    if "%" not in f[:k]:
                        out.append((f[k:], kinds))
            return dedupe(out)
        return []

    def int_values(self, toks, depth=0):
        """The integer literals an expression can be (a ternary of them, a
        variable assigned them), or None."""
        toks = strip_expr(toks)
        if depth > 4 or not toks:
            return None
        if len(toks) == 1 and toks[0].kind == "number" and toks[0].text.isdigit():
            return [int(toks[0].text)]
        tern = ternary(toks)
        if tern:
            a = self.int_values(tern[1], depth + 1)
            b = self.int_values(tern[2], depth + 1)
            return a + b if a is not None and b is not None else None
        if len(toks) == 1 and toks[0].kind == "ident" and toks[0].text in self.assigns:
            out = []
            for rhs in self.assigns[toks[0].text]:
                v = self.int_values(rhs, depth + 1)
                if v is None:
                    return None
                out += v
            return sorted(set(out))
        return None


class Tail:
    """What an append can write, the same with placeholders for its
    arguments, and whether it always runs after the text it ends."""

    def __init__(self, pieces, generic, always):
        self.pieces = pieces
        self.generic = generic
        self.always = always


def with_tails(heads, tails, few=False):
    """Each head followed by the appends that always run after it and by
    any ordered subset of the others (they are in branches); by those that
    always run alone when the others are many (`few`: by any few of a
    handful). A combination with too many texts has placeholders for the
    arguments of its most varied appends."""
    if not heads:
        return []
    optional = [t for t in tails if not t.always]
    if len(optional) > (2 * MAX_APPENDS if few else MAX_APPENDS):
        optional = []
    out = []
    # (after a placeholder, one more: the appends are the whole text)
    for r in range(min(len(optional), MAX_APPENDS + few) + 1):
        for chosen in itertools.combinations(optional, r):
            used = [t for t in tails if t.always or t in chosen]
            parts = [heads] + [t.pieces for t in used]
            while product_size(parts) > MAX_DERIVED:
                k = max(range(1, len(parts)), key=lambda c: len(parts[c]), default=None)
                if k is None or not 0 < len(used[k - 1].generic) < len(parts[k]):
                    break
                parts[k] = used[k - 1].generic
            if product_size(parts) > MAX_DERIVED:
                continue
            for combo in itertools.product(*parts):
                out.append(("".join(p for p, _ in combo), [x for _, ks in combo for x in ks]))
    return dedupe(out)[:MAX_HELD]


def writable(param):
    """Is a parameter declaration a `char *` (or `char name[]`) the
    function may write?"""
    words = [t.text for t in param]
    return "char" in words and ("*" in words or "[" in words) and "const" not in words


def product_size(choices):
    n = 1
    for c in choices:
        n *= max(1, len(c))
    return n


def expression_end(toks, i, end):
    """The index of the `;` or top-level `,` ending the expression at i."""
    depth = 0
    while i < end:
        x = toks[i]
        if x.kind == "punct":
            if x.text in "([{":
                depth += 1
            elif x.text in ")]}":
                if depth == 0:
                    break
                depth -= 1
            elif x.text in (";", ",") and depth == 0:
                break
        i += 1
    return i


# --------------------------------------------------------- the catalog

class Entry:
    def __init__(self, fmt):
        self.fmt = fmt
        self.uses = set()
        self.args = None  # per conversion: set of kinds
        self.sites = []
        self.bases = []
        self.expanded = False


def entry_id(fmt):
    return hashlib.sha1(fmt.encode("utf-8")).hexdigest()[:12]


def worth_keeping(fmt):
    """A format with words, or with conversions between punctuation that
    tells its parts apart ("%c - %s.", "%s (%d)"); not one of conversions
    and sentence punctuation only ("%s.", "%s, %s"), which matches
    anything."""
    literal = CONV_RE.sub("", fmt)
    if re.search(r"[A-Za-z]", literal):
        return True
    return bool(conversions(fmt)) and re.sub(r"[\s.,!?;:'\"]", "", literal) != ""


class Catalog:
    def __init__(self):
        self.entries = {}

    def add(self, fmt, use, site, kinds, base=None, force=False):
        """kinds: one kind per conversion of fmt. `force` keeps a format
        that is not worth keeping on its own: the generic format of
        derived entries, which P7 sends ("%s %s%s%s")."""
        if not fmt or not (worth_keeping(fmt) or force):
            return None
        n = len(conversions(fmt))
        assert len(kinds) == n, (fmt, kinds, site)
        e = self.entries.get(fmt)
        if e is None:
            e = self.entries[fmt] = Entry(fmt)
        e.uses.add(use)
        if e.args is None:
            e.args = [set() for _ in kinds]
        for s, k in zip(e.args, kinds):
            s.update(k.split("|"))
        if site not in e.sites:
            e.sites.append(site)
        if base and base != fmt and base in self.entries and base not in e.bases:
            e.bases.append(base)
        return e

    def add_all(self, use, site, generic, variants, prefixes=("",), suffix=""):
        """A call's generic format and the texts it derives, with each
        prefix vpline puts before them."""
        for prefix in prefixes:
            g = prefix + generic[0] + suffix
            derives = any(prefix + f + suffix != g and worth_keeping(prefix + f + suffix)
                          for f, _ in variants)
            # a format of conversions alone stays as the key P7 sends, when
            # it has entries derived from it
            base = self.add(g, use, site, generic[1], force=derives and bool(conversions(g)))
            # a derived entry's site is short: its base shows the call
            short = site.split("(", 1)[0].rsplit(" ", 1)[0] if base is not None else site
            for fmt, kinds in variants:
                full = prefix + fmt + suffix
                if full == g:
                    continue
                self.add(full, use, short, kinds, base=g if base else None)
                if base is not None:
                    base.expanded = True


class LayoutCatalog:
    """The catalog seen from a layout function: everything it adds is
    used as layout."""

    def __init__(self, cat):
        self.cat = cat

    def add(self, fmt, use, site, kinds, base=None, force=False):
        return self.cat.add(fmt, "layout", site, kinds, base, force)

    def add_all(self, use, site, generic, variants, prefixes=("",), suffix=""):
        return self.cat.add_all("layout", site, generic, variants, prefixes, suffix)


def add_rip(cat, unit):
    """The tombstone's lines (rip_txt), as layout: each line as it is, and
    a line with text centered between its sides as a pattern."""
    for line in unit.arrays.get("rip_txt", []):
        site = f"src/{unit.path}:0 rip_txt"
        cat.add(escape(line), "layout", site, [], force=True)
        m = RIP_SIDE.match(line)
        if m:
            cat.add(escape(m.group(1)) + "|%s|" + escape(m.group(3)), "layout", site, ["text"], force=True)


def add_killers(cat, contexts):
    """The death text formatkiller() writes (topten.c) by hand, its prefix
    then the killer with a loop of its own: "killed by %s", "choked on
    %s"..., and ", while %s" when the hero was helpless ("killed by a
    jackal, while fainted from lack of food"). The tombstone, #overview's
    graves and a grave's text show it."""
    for ctx in contexts:
        if ctx.unit.path != "topten.c" or ctx.func.name != "formatkiller":
            continue
        site = f"src/topten.c:{ctx.unit.toks[ctx.func.body[0]].line} formatkiller"
        killer = "monster|object|text"
        for prefix in dedupe([p for p in ctx.arrays.get("killed_by_prefix", []) if p]):
            head = escape(prefix) + "%s"
            cat.add(head, "death", site, [killer])
            cat.add(head + ", while %s", "death", site, [killer, "text"])
            cat.add(head + ", while helpless", "death", site, [killer])
    # dokick.c kickstr(): strcat(strcpy(buf, "kicking "), what)
    for ctx in contexts:
        if ctx.unit.path == "dokick.c" and ctx.func.name == "kickstr":
            line = ctx.unit.toks[ctx.func.body[0]].line
            cat.add("kicking %s", "death", f"src/dokick.c:{line} kickstr", ["object|text"])
    # #overview's grave of the hero says it of "you" (dungeon.c, strsubst
    # of the first " himself", " herself", " his ", " her "): "killed by
    # your own player", "killed yourself with your bullwhip"
    for e in [e for e in cat.entries.values() if "death" in e.uses]:
        text = " " + e.fmt
        for old, new in ((" himself", " yourself"), (" herself", " yourself"),
                         (" his ", " your "), (" her ", " your ")):
            text = text.replace(old, new, 1)
        if text[1:] != e.fmt:
            kinds = ["|".join(sorted(k)) for k in (e.args or [])]
            cat.add(text[1:], "death", "src/dungeon.c:3711 print_mapseen", kinds)


def add_appended(cat, contexts):
    """What a helper of APPENDED_BY appends, as pieces of their own: the
    appends that always run, then one of those in a branch ("esteemed" +
    " sir"), the argument of "For you, %s; only ..." """
    for ctx in contexts:
        if ctx.func.name not in APPENDED_BY or not ctx.param_list:
            continue
        param = ctx.param_list[0]
        start = ctx.func.body[0]
        tails = [ctx.tail(op, start + 1, 1) for op in ctx.ops.get(param, [])
                 if op.append and not op.transform]
        heads = [("", [])]
        for t in tails:
            if t.always:
                heads = [(f + g, k + h) for f, k in heads for g, h in t.pieces]
        branches = [p for t in tails if not t.always for p in t.pieces]
        site = f"src/{ctx.unit.path}:{ctx.unit.toks[start].line} {ctx.func.name}"
        for f, k in heads:
            for g, h in branches or [("", [])]:
                cat.add(f + g, "sprintf", site, k + h)


EXTCMD_ROW = re.compile(r" %-(\d+)s %4s %s")


def add_extcmds(cat, units):
    """The extended commands' descriptions (cmd.c extcmdlist[]: "apply
    (use) a tool (pick-axe, key, lamp...)"), which the `#` palette and the
    `#?` list show, and each command's row of the `#?` list."""
    for unit in units:
        if unit.path != "cmd.c":
            continue
        toks = unit.toks
        # doextlist's " %-14s %4s %s": name, flags, description
        rows = [(t.line, int(m.group(1))) for t in toks if t.kind == "string"
                for m in [EXTCMD_ROW.fullmatch(t.value)] if m]
        for i, t in enumerate(toks):
            if not (t.text == "extcmdlist" and toks[i + 1].text == "["
                    and toks[i + 3].text == "=" and toks[i + 4].text == "{"):
                continue
            close = match_close(toks, i + 4)
            j = i + 5
            while j < close:
                if toks[j].text != "{":
                    j += 1
                    continue
                end = match_close(toks, j)
                row = split_args(toks, j, end)
                if len(row) >= 3 and row[2] and all(x.kind == "string" for x in row[2]):
                    name = "".join(x.value for x in row[1]) if row[1] and all(
                        x.kind == "string" for x in row[1]) else "?"
                    desc = "".join(x.value for x in row[2])
                    cat.add(escape(desc), "menu", f"src/cmd.c:{toks[j].line} extcmdlist {name}", [])
                    for line, width in rows:
                        add_extcmd_row(cat, f"src/cmd.c:{line} doextlist {name}", name, desc, width)
                j = end + 1


def add_extcmd_row(cat, site, name, desc, width):
    """A command's row of the `#?` list with its name in place, as it is: a
    format of conversions and spaces alone is found by no text. The
    description is a piece, translated on its own; after a name of no
    letters ("?"), which anchors nothing, it stays in the row. A game in
    neither wizard nor explore mode lists #genocided as only genocided."""
    head = " " + escape(name.ljust(width)) + " %4s "
    if " been genocided or become extinct" in desc:
        short = desc.replace(" been genocided or become extinct", " been genocided")
        cat.add(escape(short), "menu", site, [])
    if re.search(r"[A-Za-z]", name):
        cat.add(head + "%s", "menu", site, ["text", "text"])
    else:
        cat.add(head + escape(desc), "menu", site, ["text"])


OPTION_MACROS = ("NHOPTB", "NHOPTC", "NHOPTP", "NHOPTO")


def add_options(cat, units):
    """What 'O' shows of the options: their descriptions (the last argument
    of each NHOPTB/C/P/O() of include/optlist.h: "can your character hear
    anything"), as its help lists them; an "other setting"'s name
    ("autopickup exceptions"); the sections of its menu ("General")."""
    with open(os.path.join(UPSTREAM, "include", "optlist.h"), encoding="latin-1") as f:
        toks = clex.scan_unit("optlist.h", f.read()).toks
    for i, t in enumerate(toks):
        if t.kind != "ident" or t.text not in OPTION_MACROS or toks[i + 1].text != "(":
            continue
        args = split_args(toks, i + 1, match_close(toks, i + 1))
        other = t.text == "NHOPTO"
        name = source_text(args[2] if other else args[0])
        for arg in [args[-1]] + ([args[0]] if other else []):
            if arg and all(x.kind == "string" for x in arg):
                text = "".join(x.value for x in arg)
                cat.add(escape(text), "sprintf", f"include/optlist.h:{t.line} {name}", [])
    for unit in units:
        for section in unit.arrays.get("OptS_type", []) if unit.path == "options.c" else []:
            cat.add(escape(section), "sprintf", "src/options.c:0 OptS_type", [])


def site_text(path, func, line, call, args):
    shown = ", ".join(source_text(a) for a in args)
    if len(shown) > 120:
        shown = shown[:117] + "..."
    return f"{path}:{line} {func} {call}({shown})"


def scan_function(cat, ctx):
    glob, unit, func = ctx.glob, ctx.unit, ctx.func
    toks = unit.toks
    if func.name in LAYOUT_FUNCS:
        cat = LayoutCatalog(cat)
    start, end = func.body
    path = "src/" + unit.path
    i = start
    while i < end:
        t = toks[i]
        if t.kind == "ident" and t.text in ASSIGN_SINKS and toks[i + 1].text == "=" and toks[i - 1].text in (".", "->"):
            j = expression_end(toks, i + 2, end)
            rhs = toks[i + 2:j]
            v = ctx.values(rhs)
            site = f"{path}:{t.line} {func.name} {t.text} = {source_text(rhs)}"
            for x in v or []:
                cat.add(escape(x), ASSIGN_SINKS[t.text], site, [])
            i = j
            continue
        if t.kind != "ident" or toks[i + 1].text != "(" or toks[i - 1].text in (".", "->"):
            i += 1
            continue
        name = t.text
        close = match_close(toks, i + 1)
        if name in NOT_SHOWN:
            i = close + 1
            continue
        args = split_args(toks, i + 1, close)
        site = site_text(path, func.name, t.line, name, args)
        if name in PLINE1 and args:
            _, prefixes = PLINE[PLINE1[name]]
            add_printf(cat, ctx, "pline", None, args, i, site, prefixes, PLINE1[name])
        elif name in PLINE and len(args) > PLINE[name][0]:
            fidx, prefixes = PLINE[name]
            add_printf(cat, ctx, PLINE_USE.get(name, "pline"), args[fidx], args[fidx + 1:], i,
                       site, prefixes, name)
        elif name in BUFFER_OPS and len(args) > BUFFER_OPS[name][1]:
            didx, tidx, printf = BUFFER_OPS[name]
            dest, _ = destination(args[didx])
            use = "death" if dest and "killer" in dest else "sprintf"
            if printf:
                add_printf(cat, ctx, use, args[tidx], args[tidx + 1:], i, site, ("",), name)
            else:
                add_text(cat, ctx, use, args[tidx], i, site)
        elif name in SINKS:
            for use, tidx in SINKS[name]:
                if len(args) > tidx:
                    add_text(cat, ctx, use, args[tidx], i, site)
        elif name in ENL:
            add_enlightenment(cat, ctx, name, args, i, site)
        i += 1


def add_printf(cat, ctx, use, fmt_toks, args, pos, site, prefixes, name):
    """A printf-style call. fmt_toks None: the X1(cstr) shorthands."""
    suffix = '"' if name == "verbalize" else ""
    if fmt_toks is None:
        fmts = [("%s", None)]
    else:
        fmts = ctx.formats(fmt_toks, pos)
    for fmt, kinds in fmts:
        if kinds is not None:
            # a buffer as the format: what it holds is the text, its %%s
            # the conversions the call's arguments fill
            cat.add_all(use, site, as_format(ctx, fmt, kinds, args), [], prefixes, suffix)
            continue
        cat.add_all(use, site, ctx.generic(fmt, args), ctx.variants(fmt, args, pos), prefixes, suffix)
        for text in dedupe(ctx.left_out):
            cat.add(text, "sprintf", site, [])
        ctx.left_out = []


def as_format(ctx, held, kinds, args):
    """The format a buffer holds: `held` is its text as a format (its %
    escaped as %%, its unknown parts %s with `kinds`); used as a format,
    each %% before a conversion ("That %s is %%s!") becomes a conversion
    the call's arguments fill."""
    out = []
    out_kinds = []
    k = 0
    ai = 0
    i = 0
    while i < len(held):
        if held.startswith("%%", i):
            m = CONV_RE.match(held, i + 1)
            if m and m.group("conv") != "%":
                ai += (m.group("width") == "*") + (m.group("prec") == "*")
                arg = args[ai] if ai < len(args) else []
                ai += 1
                out.append(m.group(0))
                out_kinds.append(ctx.kind(arg) if conv_kind(m) == "text" else conv_kind(m))
                i = m.end()
                continue
            out.append("%%")
            i += 2
            continue
        m = CONV_RE.match(held, i) if held[i] == "%" else None
        if m:
            out.append(m.group(0))
            out_kinds.append(kinds[k] if k < len(kinds) else "text")
            k += 1
            i = m.end()
            continue
        out.append(held[i])
        i += 1
    return "".join(out), out_kinds


def add_text(cat, ctx, use, arg, pos, site):
    """Text that is not a format: each literal it can be (a ternary's
    literal side, what callers pass for a parameter), or what the buffer it
    names holds."""
    for fmt, kinds in ctx.pieces(arg, pos, 0):
        cat.add(fmt, use, site, kinds)


def add_enlightenment(cat, ctx, name, args, pos, site):
    """you_are(attr, ps) and its kin: " You are <attr><ps>." while the game
    goes on, " You were <attr><ps>." at its end; enl_msg(prefix, present,
    past, suffix, ps) the same with its own words."""
    lit = lambda text: [(escape(text), [])]  # noqa: E731
    if name == "enl_msg":
        if len(args) != 5:
            return
        prefixes = ctx.pieces(args[0], pos, 0)
        verbs = ctx.pieces(args[1], pos, 0) + ctx.pieces(args[2], pos, 0)
        attr, ps = args[3], args[4]
    else:
        spec = ENL[name]
        if name in ("you_have_been", "you_have_never", "you_have_X"):
            if len(args) != 1:
                return
            attr, ps = args[0], None
        else:
            if len(args) != 2:
                return
            attr, ps = args
        prefixes = lit(spec[0])
        verbs = lit(spec[1]) + lit(spec[2])
    parts = [prefixes, dedupe(verbs), ctx.pieces(attr, pos, 0),
             ctx.pieces(ps, pos, 0) if ps is not None else lit("")]
    # (the present and the past double every line)
    while product_size(parts) > 2 * MAX_DERIVED:
        k = max(range(len(parts)), key=lambda i: len(parts[i]))
        # what the line no longer spells out is a piece of its own
        # ("%s (current; limit:%s)")
        for piece, kinds in parts[k]:
            cat.add(piece, "sprintf", site, kinds)
        parts[k] = [("%s", ["text"])]
    for combo in itertools.product(*parts):
        fmt = " " + "".join(p for p, _ in combo) + "."
        for twowords, short in ENL_CONTRACTIONS:
            fmt = fmt.replace(twowords, short)
        cat.add(fmt, "window", site, [k for _, ks in combo for k in ks])


# ------------------------------------------------------------ the run

def load_globals(units):
    glob = Globals()
    inc = os.path.join(UPSTREAM, "include")
    for name in sorted(os.listdir(inc)):
        if not name.endswith(".h"):
            continue
        with open(os.path.join(inc, name), encoding="latin-1") as f:
            u = clex.scan_unit(name, f.read())
        glob.strings.update(u.strings)
        glob.arrays.update(u.arrays)
        glob.structs.update(u.structs)
    with open(os.path.join(inc, "extern.h"), encoding="latin-1") as f:
        toks = clex.lex(f.read())
    for i, t in enumerate(toks):
        if t.kind == "ident" and i + 1 < len(toks) and toks[i + 1].text == "(" and toks[i - 1].text != "(":
            close = match_close(toks, i + 1)
            if close + 1 < len(toks) and toks[close + 1].text in (";", "NONNULL", "NONNULLARG1", "NO_NNARGS",
                                                                   "NONNULLARG12", "NONNULLARG2", "NORETURN",
                                                                   "PRINTF_F", "NONNULLARG123", "NONNULLPTRS"):
                glob.prototypes.setdefault(t.text, [writable(a) for a in split_args(toks, i + 1, close)])
    # the common strings: #define nothing_happens c_common_strings.c_nothing_happens
    decl = next(u for u in units if u.path == "decl.c")
    with open(os.path.join(inc, "decl.h"), encoding="latin-1") as f:
        decl_h = f.read()
    for struct in ("c_common_strings",):
        fields = struct_fields(os.path.join(inc, "hack.h"), struct)
        table = dict(zip(fields, struct_initializer(decl, struct)))
        for m in re.finditer(rf"#define\s+(\w+)\s+{struct}\.(\w+)\s*$", decl_h, re.M):
            v = table.get(m.group(2))
            if isinstance(v, str):
                glob.strings[m.group(1)] = v
    for u in units:
        for k, v in u.arrays.items():
            glob.arrays.setdefault(k, v)
        for k, v in u.structs.items():
            glob.structs.setdefault(k, v)
        for k, v in u.tables.items():
            glob.tables.setdefault(k, v)
    return glob


def struct_fields(header, struct):
    with open(header, encoding="latin-1") as f:
        src = f.read()
    m = re.search(rf"struct {struct} {{(.*?)}};", src, re.S)
    return [n for n, _ in re.findall(r"\*const\s+(\w+)(\[\d+\])?", m.group(1))]


def struct_initializer(unit, struct):
    toks = unit.toks
    for i, t in enumerate(toks):
        if t.text == struct and toks[i + 1].text == "=" and toks[i + 2].text == "{":
            close = match_close(toks, i + 2)
            out = []
            for arg in split_args(toks, i + 2, close):
                if arg and all(x.kind == "string" for x in arg):
                    out.append("".join(x.value for x in arg))
                else:
                    out.append(None)
            return out
    return []


def extract():
    src_dir = os.path.join(UPSTREAM, "src")
    units = []
    for name in sorted(os.listdir(src_dir)):
        if name.endswith(".c"):
            with open(os.path.join(src_dir, name), encoding="latin-1") as f:
                units.append(clex.scan_unit(name, f.read()))
    glob = load_globals(units)
    contexts = [Context(glob, unit, func) for unit in units for func in unit.functions]
    for ctx in contexts:
        glob.index(ctx)
    cat = Catalog()
    for ctx in contexts:
        scan_function(cat, ctx)
    for unit in units:
        if unit.path == "rip.c":
            add_rip(cat, unit)
    add_options(cat, units)
    add_extcmds(cat, units)
    add_killers(cat, contexts)
    add_appended(cat, contexts)
    for fmt, use, site, kinds in datfiles.extract(UPSTREAM):
        cat.add(fmt, use, site, kinds)
    return cat


def site_key(site):
    m = re.match(r"(\S+?):(\d+)", site)
    return (m.group(1), int(m.group(2))) if m else (site, 0)


def render(cat):
    entries = sorted(cat.entries.values(), key=lambda e: (site_key(e.sites[0]), e.fmt))
    ids = {}
    for e in entries:
        i = entry_id(e.fmt)
        if i in ids:
            raise SystemExit(f"id collision: {ids[i]!r} and {e.fmt!r}")
        ids[i] = e.fmt
    lines = []
    for e in entries:
        obj = {"id": entry_id(e.fmt), "fmt": e.fmt, "uses": sorted(e.uses)}
        if e.args:
            obj["args"] = ["|".join(sorted(a)) for a in e.args]
        if e.bases:
            # every generic format it derives from (P7 sends any of them)
            obj["from"] = [entry_id(b) for b in e.bases]
        if e.expanded:
            obj["expanded"] = True
        obj["sites"] = e.sites
        lines.append(json.dumps(obj, ensure_ascii=False))
    head = {"format": CATALOG_FORMAT, "engine": ENGINE, "count": len(lines)}
    return json.dumps(head)[:-1] + ', "entries": [\n' + ",\n".join(lines) + "\n]}\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="fail if the catalog is out of date")
    ap.add_argument("--out", default=OUT)
    opts = ap.parse_args()
    text = render(extract())
    if opts.check:
        with open(opts.out, encoding="utf-8") as f:
            if f.read() != text:
                raise SystemExit(f"{opts.out} is out of date: run tools/i18n/extract.py")
        print(f"{opts.out} is up to date")
        return
    os.makedirs(os.path.dirname(opts.out), exist_ok=True)
    with open(opts.out, "w", encoding="utf-8") as f:
        f.write(text)
    print(f"{opts.out}: {len(cat_lines(text))} entries")


def cat_lines(text):
    return [line for line in text.split("\n")[1:] if line.startswith("{")]


if __name__ == "__main__":
    main()
