"""A small C lexer and scanner for the extractor: tokens, preprocessor
directives, function bodies and calls. Not a compiler: it reads NetHack's
sources well enough to find every call that shows text, the function it is
in and its arguments.

`#if 0` blocks are skipped; every other conditional branch is read (a
catalog may hold strings a build leaves out, it must not miss any).
"""

import re
from dataclasses import dataclass, field

TOKEN_RE = re.compile(
    r"""
    (?P<ws>[ \t\r\f\v]+|\\\n)
  | (?P<nl>\n)
  | (?P<comment>/\*.*?\*/|//[^\n]*)
  | (?P<string>L?"(?:\\.|[^"\\\n])*")
  | (?P<char>L?'(?:\\.|[^'\\\n])+')
  | (?P<number>\.?\d(?:[eEpP][+-]|[\w.])*)
  | (?P<ident>[A-Za-z_]\w*)
  | (?P<punct>->|\+\+|--|<<=|>>=|<<|>>|<=|>=|==|!=|&&|\|\||[-+*/%&|^]=|\.\.\.|\#\#|[^\s\w])
    """,
    re.S | re.X,
)

ESCAPES = {
    "n": "\n", "t": "\t", "r": "\r", "a": "\a", "b": "\b", "f": "\f",
    "v": "\v", "\\": "\\", "'": "'", '"': '"', "?": "?",
}


def decode_c_string(body):
    """The value of a C string or character literal's body (no quotes)."""
    out = []
    i = 0
    while i < len(body):
        c = body[i]
        if c != "\\":
            out.append(c)
            i += 1
            continue
        i += 1
        if i >= len(body):
            break
        e = body[i]
        if e in ESCAPES:
            out.append(ESCAPES[e])
            i += 1
        elif e in "01234567":
            j = i
            while j < len(body) and j < i + 3 and body[j] in "01234567":
                j += 1
            out.append(chr(int(body[i:j], 8)))
            i = j
        elif e == "x":
            j = i + 1
            while j < len(body) and body[j] in "0123456789abcdefABCDEF":
                j += 1
            out.append(chr(int(body[i + 1 : j] or "0", 16)))
            i = j
        else:
            out.append(e)
            i += 1
    return "".join(out)


@dataclass
class Tok:
    kind: str  # ident, string, char, number, punct, pp (a directive)
    text: str
    line: int
    value: str = ""  # a string's or char's decoded value; a directive's text


def lex(src):
    """Tokens of a C file; directives become `pp` tokens; `#if 0` blocks
    are dropped."""
    toks = []
    pos = 0
    line = 1
    bol = True
    skip = []  # per open conditional: is this branch skipped?
    n = len(src)
    while pos < n:
        if bol:
            m = re.compile(r"[ \t]*#").match(src, pos)
            if m:
                # a directive: up to the newline not escaped by a backslash
                end = pos
                text = []
                while end < n:
                    nl = src.find("\n", end)
                    if nl < 0:
                        nl = n
                    seg = src[end:nl]
                    if seg.endswith("\\"):
                        text.append(seg[:-1])
                        end = nl + 1
                        line += 1
                        continue
                    text.append(seg)
                    end = nl
                    break
                d = " ".join(text)
                # comments spanning lines inside a directive are rare;
                # a /* left open swallows the lines up to its end
                while "/*" in d and "*/" not in d[d.index("/*") :]:
                    nl = src.find("*/", end)
                    if nl < 0:
                        break
                    line += src.count("\n", end, nl + 2)
                    d += src[end : nl + 2]
                    end = nl + 2
                d = re.sub(r"/\*.*?\*/", " ", d, flags=re.S)
                d = re.sub(r"//.*", "", d).strip()
                directive_line = line
                pos = end
                _conditional(d, skip)
                if not any(skip):
                    toks.append(Tok("pp", d, directive_line, d))
                continue
        m = TOKEN_RE.match(src, pos)
        if not m:
            raise ValueError(f"cannot lex at line {line}: {src[pos:pos + 20]!r}")
        kind = m.lastgroup
        text = m.group(kind)
        pos = m.end()
        if kind == "nl":
            line += 1
            bol = True
            continue
        if kind == "ws":
            if text == "\\\n":
                line += 1
            continue
        if kind == "comment":
            line += text.count("\n")
            continue
        bol = False
        if any(skip):
            continue
        if kind == "string":
            toks.append(Tok("string", text, line, decode_c_string(text[text.index('"') + 1 : -1])))
        elif kind == "char":
            toks.append(Tok("char", text, line, decode_c_string(text[text.index("'") + 1 : -1])))
        else:
            toks.append(Tok(kind, text, line))
    return toks


def _conditional(d, skip):
    """Track `#if 0` ... `#else` ... `#endif`: skip[i] says whether the
    current branch of the i-th open conditional is skipped."""
    m = re.match(r"#\s*(\w+)\s*(.*)", d)
    if not m:
        return
    word, rest = m.group(1), m.group(2).strip()
    if word in ("if", "ifdef", "ifndef"):
        skip.append(word == "if" and rest in ("0", "(0)"))
    elif word in ("else", "elif"):
        if skip:
            # the branch after `#if 0` is read; any branch after a read
            # one is read too (all branches of a real condition count)
            skip[-1] = False
    elif word == "endif":
        if skip:
            skip.pop()


def match_close(toks, i):
    """The index of the bracket closing the one at toks[i]."""
    opening = toks[i].text
    closing = {"(": ")", "[": "]", "{": "}"}[opening]
    depth = 0
    for j in range(i, len(toks)):
        t = toks[j].text
        if toks[j].kind != "punct":
            continue
        if t == opening:
            depth += 1
        elif t == closing:
            depth -= 1
            if depth == 0:
                return j
    return len(toks) - 1


def split_args(toks, open_i, close_i):
    """The arguments between the parentheses at open_i and close_i, split at
    their top-level commas, as lists of tokens."""
    args = []
    cur = []
    depth = 0
    for t in toks[open_i + 1 : close_i]:
        if t.kind == "punct":
            if t.text in "([{":
                depth += 1
            elif t.text in ")]}":
                depth -= 1
            elif t.text == "," and depth == 0:
                args.append(cur)
                cur = []
                continue
        cur.append(t)
    if cur or args:
        args.append(cur)
    return args


NO_SPACE_BEFORE = {")", "]", ",", ";", ".", "->", "++", "--"}
NO_SPACE_AFTER = {"(", "[", ".", "->", "!", "~"}


def source_text(toks):
    """An expression's tokens as compact C text (for people to read)."""
    out = []
    for i, t in enumerate(toks):
        if i > 0:
            prev = toks[i - 1]
            tight = (
                t.text in NO_SPACE_BEFORE
                or prev.text in NO_SPACE_AFTER
                or (t.text in ("(", "[") and prev.kind == "ident")
            )
            if not tight:
                out.append(" ")
        out.append(t.text)
    return "".join(out)


@dataclass
class Function:
    name: str
    body: tuple  # (index of "{", index of "}")
    line: int


@dataclass
class Unit:
    """A lexed source file and what is found in it at the top level."""

    path: str
    toks: list
    functions: list = field(default_factory=list)
    # name -> list of string values: `static const char *const x[] = {...}`
    # (top level), keyed by the array's name
    arrays: dict = field(default_factory=dict)
    # name -> value: `#define NAME "literal"` and `static const char x[] = "..."`
    strings: dict = field(default_factory=dict)
    # struct tag -> its members' names, in order
    structs: dict = field(default_factory=dict)
    # name -> (struct tag, rows): `struct tag name[] = { {...}, {...} }`,
    # each row its initializer's elements as token lists
    tables: dict = field(default_factory=dict)


ATTRIBUTE_WORDS = {"NORETURN", "UNUSED", "NONNULL", "NONNULLARG1", "NONNULLARG2",
                   "NONNULLARG12", "NONNULLARG3", "NONNULLARG123", "NONNULLPTRS",
                   "NO_NNARGS", "PRINTF_F", "PRINTF_F_PTR", "ATTRNORETURN"}


def scan_unit(path, src):
    toks = lex(src)
    unit = Unit(path, toks)
    n = len(toks)
    i = 0
    while i < n:
        t = toks[i]
        if t.kind == "pp":
            m = re.match(r"#\s*define\s+(\w+)\s+(.*)$", t.value)
            if m:
                value = string_literal_value(m.group(2))
                if value is not None:
                    unit.strings[m.group(1)] = value
            i += 1
            continue
        if t.kind == "ident" and i + 1 < n and toks[i + 1].text == "(":
            close = match_close(toks, i + 1)
            k = close + 1
            # attribute macros between a definition's ')' and its '{'
            while k < n and toks[k].kind == "ident" and toks[k].text in ATTRIBUTE_WORDS:
                k += 1
                if k < n and toks[k].text == "(":
                    k = match_close(toks, k) + 1
            if k < n and toks[k].text == "{":
                end = match_close(toks, k)
                unit.functions.append(Function(t.text, (k, end), t.line))
                i = end + 1
                continue
            i = close + 1
            continue
        if t.text == "=" and i + 1 < n and toks[i + 1].text == "{":
            # a top-level initializer: an array of strings is a table, an
            # array of structs a table of rows
            end = match_close(toks, i + 1)
            name = declared_name(toks, i)
            values = string_array(toks[i + 2 : end])
            if name and values is not None:
                unit.arrays[name] = values
            elif name:
                rows = struct_rows(toks, i + 1, end)
                tag = declared_struct(toks, i)
                if rows and tag:
                    unit.tables[name] = (tag, rows)
            i = end + 1
            continue
        if t.text == "=" and i + 1 < n and toks[i + 1].kind == "string":
            name = declared_name(toks, i)
            j = i + 1
            parts = []
            while j < n and toks[j].kind == "string":
                parts.append(toks[j].value)
                j += 1
            if name and j < n and toks[j].text in (";", ","):
                unit.strings[name] = "".join(parts)
            i = j
            continue
        if t.text == "{":
            # struct, union or enum bodies at the top level
            close = match_close(toks, i)
            if i >= 2 and toks[i - 2].text == "struct" and toks[i - 1].kind == "ident":
                unit.structs[toks[i - 1].text] = struct_members(toks[i + 1 : close])
            i = close + 1
            continue
        i += 1
    return unit


def struct_members(toks):
    """The names of a struct body's members, in order."""
    names = []
    decl = []
    depth = 0
    for t in toks:
        if t.text in "([{":
            depth += 1
        elif t.text in ")]}":
            depth -= 1
        if depth == 0 and t.text in (";", ","):
            name = declarator_name(decl)
            if name:
                names.append(name)
            # `const char *a, *b;`: the type stays for the next one
            decl = [] if t.text == ";" else [x for x in decl if x.kind == "ident"][:1]
            continue
        decl.append(t)
    return names


def declarator_name(decl):
    """`const char *name`, `char name[N]`, `int (*fn)(void)`, `unsigned x:1`."""
    for j in range(len(decl) - 2):
        if decl[j].text == "(" and decl[j + 1].text == "*" and decl[j + 2].kind == "ident":
            return decl[j + 2].text
    depth = 0
    name = None
    for t in decl:
        if t.text in "([":
            depth += 1
        elif t.text in ")]":
            depth -= 1
        elif t.text == ":" and depth == 0:
            break
        elif t.kind == "ident" and depth == 0:
            name = t.text
    return name


def struct_rows(toks, open_i, close_i):
    """The rows of `{ {a, b}, {c, d} }`: each a list of element token lists;
    None when the initializer is not made of braced rows."""
    rows = []
    for elem in split_args(toks, open_i, close_i):
        if not elem:
            continue
        if elem[0].text != "{" or match_close(elem, 0) != len(elem) - 1:
            return None
        rows.append(split_args(elem, 0, len(elem) - 1))
    return rows or None


def declared_struct(toks, eq):
    """The struct tag of the declaration whose `=` is at toks[eq]."""
    j = eq - 1
    depth = 0
    while j >= 0:
        t = toks[j]
        if t.text in (")", "]", "}"):
            depth += 1
        elif t.text in ("(", "[", "{"):
            depth -= 1
        elif depth == 0 and t.text == ";":
            break
        if depth == 0 and t.text == "struct" and j + 1 < eq and toks[j + 1].kind == "ident":
            return toks[j + 1].text
        if depth < 0:
            break
        j -= 1
    return None


def declared_name(toks, eq):
    """The name declared before the `=` at toks[eq] (`x`, `x[]`, `x[N]`)."""
    j = eq - 1
    while j >= 0 and toks[j].text == "]":
        # skip [...] groups
        depth = 0
        while j >= 0:
            if toks[j].text == "]":
                depth += 1
            elif toks[j].text == "[":
                depth -= 1
                if depth == 0:
                    break
            j -= 1
        j -= 1
    if j >= 0 and toks[j].kind == "ident":
        return toks[j].text
    return None


def string_array(toks):
    """The strings of an initializer that holds only strings (and null
    pointers), else None."""
    values = []
    cur = []
    for t in toks + [Tok("punct", ",", 0)]:
        if t.text == ",":
            if not cur:
                continue
            if all(x.kind == "string" for x in cur):
                values.append("".join(x.value for x in cur))
            elif source_text(cur) in ("0", "NULL", "(char *) 0", "(const char *) 0", "(char*)0", "(const char*)0"):
                pass
            else:
                return None
            cur = []
        else:
            cur.append(t)
    return values if values else None


def string_literal_value(text):
    """The value of a macro body made only of string literals, else None."""
    text = text.strip()
    while text.startswith("(") and text.endswith(")"):
        text = text[1:-1].strip()
    parts = re.findall(r'"((?:\\.|[^"\\])*)"', text)
    if not parts or re.sub(r'"(?:\\.|[^"\\])*"', "", text).strip():
        return None
    return "".join(decode_c_string(p) for p in parts)
