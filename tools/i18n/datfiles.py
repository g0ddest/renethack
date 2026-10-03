"""The texts of engine/upstream/dat for the catalog: rumours, oracles,
epitaphs, engravings, hallucinatory monster names, the quest texts of
quest.lua and the messages and engravings of the level files.

Not dat/tribute nor dat/data.base (third-party copyright).

Quest codes (%p the hero's name, %l the leader, %dC a capitalised god...)
become %s placeholders whose kind says the code: "quest:p", "quest:dC".
"""

import os
import re

# the quest text codes of questpgr.c convert_arg() and their suffixes
QUEST_CODES = "pcrRsSlionOgGHaAdDCNLxZ"
QUEST_SUFFIXES = "AaCPpSst"
# pronoun suffixes, only after %d %l %n %o
QUEST_PRONOUNS = "hHiIjJ"


def quest_format(text):
    """A quest text as a printf format: (fmt, kinds)."""
    out = []
    kinds = []
    i = 0
    while i < len(text):
        c = text[i]
        if c == "%" and i + 1 < len(text):
            code = text[i + 1]
            if code == "%":
                out.append("%%")
                i += 2
                continue
            if code in QUEST_CODES:
                suffix = text[i + 2] if i + 2 < len(text) else ""
                if suffix in QUEST_SUFFIXES or (suffix in QUEST_PRONOUNS and code in "dlno"):
                    i += 3
                else:
                    suffix = ""
                    i += 2
                out.append("%s")
                kinds.append(f"quest:{code}{suffix}")
                continue
        out.append("%%" if c == "%" else c)
        i += 1
    return "".join(out), kinds


def text_lines(path):
    with open(path, encoding="latin-1") as f:
        for n, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if line.startswith("#") or not line.strip():
                continue
            yield n, line


def escape(s):
    return s.replace("%", "%%")


def extract(upstream):
    """(fmt, use, site, kinds) for every text of dat/."""
    dat = os.path.join(upstream, "dat")
    for name, use in (("rumors.tru", "rumor"), ("rumors.fal", "rumor"),
                      ("epitaph.txt", "epitaph"), ("engrave.txt", "engraving")):
        for n, line in text_lines(os.path.join(dat, name)):
            yield escape(line), use, f"dat/{name}:{n}", []
    for n, line in text_lines(os.path.join(dat, "bogusmon.txt")):
        # a leading -, _, +, | or = says the gender and whether it is a
        # personal name
        if line[0] in "-_+|=":
            line = line[1:]
        yield escape(line), "bogusmon", f"dat/bogusmon.txt:{n}", []
    yield from oracles(os.path.join(dat, "oracles.txt"))
    yield from quest_texts(os.path.join(dat, "quest.lua"))
    for name in sorted(os.listdir(dat)):
        if name.endswith(".lua") and name != "quest.lua":
            yield from level_texts(os.path.join(dat, name))


def oracles(path):
    """Passages between lines of dashes; each is shown as text-window
    lines, joined here by newlines."""
    with open(path, encoding="latin-1") as f:
        lines = f.read().split("\n")
    start = None
    for n, line in enumerate(lines + ["-----"], 1):
        if line.startswith("-----") or line.startswith("#"):
            if start is not None and n - 1 > start:
                passage = "\n".join(lines[start : n - 1]).strip("\n")
                if passage:
                    yield escape(passage), "oracle", f"dat/oracles.txt:{start + 1}", []
            start = n
            continue


# ---------------------------------------------------------------- Lua

LUA_TOKEN = re.compile(
    r"""
    (?P<ws>\s+)
  | (?P<comment>--\[(?P<ceq>=*)\[.*?\](?P=ceq)\]|--[^\n]*)
  | (?P<long>\[(?P<leq>=*)\[.*?\](?P=leq)\])
  | (?P<string>"(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*')
  | (?P<number>0[xX][0-9a-fA-F]+|\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)
  | (?P<name>[A-Za-z_]\w*)
  | (?P<op>\.\.\.|\.\.|==|~=|<=|>=|//|::|[-+*/%^\#&~|<>=(){}\[\];:,.])
    """,
    re.S | re.X,
)

LUA_ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "a": "\a", "b": "\b", "f": "\f",
               "v": "\v", "\\": "\\", '"': '"', "'": "'", "\n": "\n"}


def lua_string_value(text):
    if text.startswith("["):
        eq = re.match(r"\[(=*)\[", text).group(1)
        body = text[len(eq) + 2 : -(len(eq) + 2)]
        # a newline right after the opening bracket is skipped
        if body.startswith("\n"):
            body = body[1:]
        return body
    body = text[1:-1]
    out = []
    i = 0
    while i < len(body):
        c = body[i]
        if c == "\\" and i + 1 < len(body):
            e = body[i + 1]
            if e in LUA_ESCAPES:
                out.append(LUA_ESCAPES[e])
                i += 2
                continue
            m = re.match(r"\d{1,3}", body[i + 1 :])
            if m:
                out.append(chr(int(m.group(0))))
                i += 1 + len(m.group(0))
                continue
            out.append(e)
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


class LuaTok:
    __slots__ = ("kind", "text", "line", "value")

    def __init__(self, kind, text, line, value=None):
        self.kind, self.text, self.line, self.value = kind, text, line, value


def lua_lex(src):
    toks = []
    pos = 0
    line = 1
    while pos < len(src):
        m = LUA_TOKEN.match(src, pos)
        if not m:
            raise ValueError(f"cannot lex Lua at line {line}: {src[pos:pos + 20]!r}")
        kind = m.lastgroup
        text = m.group(0)
        if kind not in ("ws", "comment"):
            value = lua_string_value(text) if kind in ("string", "long") else None
            toks.append(LuaTok("string" if kind == "long" else kind, text, line, value))
        line += text.count("\n")
        pos = m.end()
    return toks


def lua_table(toks, i):
    """Parse the table constructor at toks[i] ('{'): (value, next index).
    A value is a str, a dict (named fields, positional ones under 1, 2...)
    or None for anything else."""
    assert toks[i].text == "{"
    i += 1
    table = {}
    pos = 1
    while toks[i].text != "}":
        key = None
        if toks[i].kind == "name" and toks[i + 1].text == "=":
            key = toks[i].text
            i += 2
        elif toks[i].text == "[":
            key = toks[i + 1].value if toks[i + 1].kind == "string" else toks[i + 1].text
            i += 4  # [ key ] =
        value, i = lua_value(toks, i)
        if key is None:
            key = pos
            pos += 1
        table[key] = value
        if toks[i].text in (",", ";"):
            i += 1
    return table, i + 1


def lua_value(toks, i):
    """A string (concatenations of literals included), a table, or None
    for an expression this reader does not evaluate."""
    if toks[i].text == "{":
        return lua_table(toks, i)
    parts = []
    depth = 0
    start = i
    while i < len(toks):
        t = toks[i]
        if depth == 0 and t.text in (",", ";", "}"):
            break
        if t.text in ("(", "{", "["):
            depth += 1
        elif t.text in (")", "}", "]"):
            depth -= 1
        parts.append(t)
        i += 1
    if parts and all(p.kind == "string" or p.text == ".." for p in parts):
        return "".join(p.value for p in parts if p.kind == "string"), i
    del start
    return None, i


def quest_texts(path):
    with open(path, encoding="latin-1") as f:
        toks = lua_lex(f.read())
    i = next(k for k, t in enumerate(toks) if t.text == "questtext")
    while toks[i].text != "{":
        i += 1
    questtext, _ = lua_table(toks, i)
    line_of = {}
    for t in toks:
        if t.kind == "string":
            line_of.setdefault(t.value, t.line)
    for section in questtext:
        if section == "msg_fallbacks":
            continue
        for msgid, msg in questtext[section].items():
            if isinstance(msg, str):
                texts = [(msg, None)]
                output = None
            elif isinstance(msg, dict):
                output = msg.get("output")
                texts = []
                if isinstance(msg.get("text"), str):
                    texts.append((msg["text"], "text"))
                if isinstance(msg.get("synopsis"), str):
                    texts.append((msg["synopsis"], "synopsis"))
                texts += [(v, None) for k, v in msg.items() if isinstance(k, int) and isinstance(v, str)]
            else:
                continue
            for text, what in texts:
                site = f"dat/quest.lua:{line_of.get(text, 0)} {section}.{msgid}"
                if what:
                    site += f".{what}"
                fmt, kinds = quest_format(text.rstrip("\n"))
                yield fmt, "quest", site, kinds
                if output == "pline" and "\n" in text.strip("\n"):
                    for line in text.strip("\n").split("\n"):
                        fmt, kinds = quest_format(line)
                        yield fmt, "quest", site, kinds


def level_texts(path):
    """des.message(), nh.pline(), nh.text() and the text of des.engraving()
    in a level file: string parts concatenated with other expressions
    (`"Use '" .. tut_key("up") .. "'"`) become %s placeholders."""
    name = os.path.basename(path)
    with open(path, encoding="latin-1") as f:
        toks = lua_lex(f.read())
    for i, t in enumerate(toks):
        if t.kind != "name" or i + 3 >= len(toks) or toks[i + 1].text != ".":
            continue
        call = f"{t.text}.{toks[i + 2].text}"
        if toks[i + 3].text != "(":
            continue
        args = lua_args(toks, i + 3)
        if call in ("des.message", "nh.pline", "nh.text") and args:
            exprs = [args[0]]
            use = "level" if name not in ("tut-1.lua", "tut-2.lua", "nhlib.lua") else "tutorial"
        elif call == "des.engraving" and args:
            exprs = engraving_texts(args)
            use = "engraving" if not name.startswith("tut-") else "tutorial"
        else:
            continue
        for expr in exprs:
            parts = concat_parts(expr)
            if parts is None:
                continue
            fmt = []
            kinds = []
            for lit in parts:
                if lit is None:
                    fmt.append("%s")
                    kinds.append("text")
                elif call == "des.message":
                    # level messages go through the quest text conversion
                    f, k = quest_format(lit)
                    fmt.append(f)
                    kinds += k
                else:
                    fmt.append(escape(lit))
            yield "".join(fmt), use, f"dat/{name}:{t.line} {call}", kinds


def lua_args(toks, open_i):
    """The arguments of the call whose '(' is at open_i, as token lists."""
    args = []
    cur = []
    depth = 0
    i = open_i + 1
    while i < len(toks):
        t = toks[i]
        if t.text in ("(", "{", "["):
            depth += 1
        elif t.text in (")", "}", "]"):
            if depth == 0:
                break
            depth -= 1
        if t.text == "," and depth == 0:
            args.append(cur)
            cur = []
        else:
            cur.append(t)
        i += 1
    if cur:
        args.append(cur)
    return args


def engraving_texts(args):
    """des.engraving({ ..., text = EXPR }) or des.engraving(coord, type,
    EXPR)."""
    if args[0] and args[0][0].text == "{":
        toks = args[0]
        for j, t in enumerate(toks):
            if t.text == "text" and j + 1 < len(toks) and toks[j + 1].text == "=":
                expr = []
                depth = 0
                for x in toks[j + 2 :]:
                    if x.text in ("(", "{", "["):
                        depth += 1
                    elif x.text in (")", "}", "]"):
                        if depth == 0:
                            break
                        depth -= 1
                    if x.text == "," and depth == 0:
                        break
                    expr.append(x)
                return [expr]
        return []
    if len(args) >= 3:
        return [args[2]]
    return []


def concat_parts(expr):
    """`"a" .. f(x) .. "b"` as ["a", None, "b"] (None: an expression this
    reader does not evaluate); None without any string."""
    parts = []
    cur = []
    depth = 0
    for t in expr + [LuaTok("op", "..", 0)]:
        if t.text in ("(", "{", "["):
            depth += 1
        elif t.text in (")", "}", "]"):
            depth -= 1
        if t.text == ".." and depth == 0:
            if len(cur) == 1 and cur[0].kind == "string":
                parts.append(cur[0].value)
            elif cur:
                parts.append(None)
            cur = []
        else:
            cur.append(t)
    if all(p is None for p in parts):
        return None
    return parts
