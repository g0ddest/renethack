#!/usr/bin/env python3
"""The in-game help (localization phase R8): NetHack's Guidebook as the
client's help panel reads it, by chapters of paragraphs.

English: the Guidebook of the engine's own sources
(engine/upstream/doc/Guidebook.mn, troff with NetHack's macros).
Russian: Vadim Velikodniy's translation, "nethack-guide" (pandoc
Markdown), fetched at a pinned commit, checked against the sha256 in
client/help/help.lock.json and kept as client/help/guidebook-ru.md.

    python3 tools/help/guidebook.py           # build both JSON files
    python3 tools/help/guidebook.py --fetch   # fetch the Russian again first
    python3 tools/help/guidebook.py --check   # fail if a JSON file is stale

Output: client/help/guidebook.{en,ru}.json, chapters in order:
    {"lang", "title", "credit", "chapters": [
        {"number": "3.1", "title": "...", "level": 2,
         "paragraphs": ["<BBCode>", ...]}]}
A paragraph is Godot RichTextLabel BBCode (a literal "[" is "[lb]"); a
search looks in it with the tags taken out. Python 3, no dependencies.
"""

import hashlib
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HELP = ROOT / "client" / "help"
LOCK = HELP / "help.lock.json"
# NetHack's sources (a submodule; another checkout's with RENETHACK_UPSTREAM)
UPSTREAM = Path(os.environ.get("RENETHACK_UPSTREAM", ROOT / "engine" / "upstream"))
GUIDEBOOK_MN = UPSTREAM / "doc" / "Guidebook.mn"
GUIDEBOOK_RU = HELP / "guidebook-ru.md"


def esc(text):
    """Text as BBCode shows it literally."""
    return text.replace("[", "[lb]")


def plain(bb):
    """BBCode without its tags: what a search looks in."""
    text = re.sub(r"\[(/?)(b|i|u|code|indent|table=\d+|table|cell|center|font_size=\d+|font_size)\]", "", bb)
    return text.replace("[lb]", "[")


class Book:
    """Chapters of paragraphs, built a piece at a time."""

    def __init__(self, lang, title, credit):
        self.lang = lang
        self.title = title
        self.credit = credit
        self.chapters = []

    def chapter(self, number, title, level):
        self.chapters.append(
            {"number": number, "title": title.strip(), "level": level, "paragraphs": []}
        )

    def para(self, bb):
        bb = bb.strip()
        if not bb or not plain(bb).strip():
            return
        if not self.chapters:
            self.chapter("", self.title, 1)
        self.chapters[-1]["paragraphs"].append(bb)

    def json(self):
        chapters = [c for c in self.chapters if c["paragraphs"] or c["level"] < 3]
        return {
            "lang": self.lang,
            "title": self.title,
            "credit": self.credit,
            "chapters": chapters,
        }


# ---- troff (Guidebook.mn) ----

# troff's special characters
CHARS = {
    "oq": "‘", "cq": "’", "lq": "“", "rq": "”", "em": "—", "en": "–",
    "ha": "^", "dq": '"', "ti": "~", "bu": "•", "co": "©", "rg": "®",
    "mu": "×", "de": "°", "rs": "\\", "aq": "'", "ga": "`", "dg": "†",
    "hy": "-", "ru": "_", "lh": "☜", "rh": "☞", "+-": "±", "sc": "§",
    "or": "|", "tm": "™", "aa": "´", "->": "→", "<-": "←",
}

FONTS = {"B": "b", "I": "i", "CR": "code", "C": "code", "CW": "code", "CB": "code", "BI": "b"}


class Fonts:
    """The font in effect, as an open BBCode tag."""

    def __init__(self):
        self.name = None  # None: roman
        self.prev = None

    def _tag(self):
        return FONTS.get(self.name) if self.name else None

    def switch(self, font):
        if font == "P":
            font = self.prev
        elif font in ("R", ""):
            font = None
        out = self.close()
        self.prev, self.name = self.name, font
        return out + self.reopen()

    def close(self):
        """The tag closed (at a paragraph's end; the font stays)."""
        tag = self._tag()
        return f"[/{tag}]" if tag else ""

    def reopen(self):
        """The tag opened again (at the next paragraph's start)."""
        tag = self._tag()
        return f"[{tag}]" if tag else ""


# troff's strings (.ds) as the Guidebook defines them, by name
STRINGS = {"rg": "®", "tm": "™"}


def troff_inline(text, fonts):
    """A line of troff text: escapes to characters, fonts to BBCode."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c != "\\":
            out.append(esc(c))
            i += 1
            continue
        if i + 1 >= n:
            break
        e = text[i + 1]
        i += 2
        if e == "f":
            if i < n and text[i] == "(":
                font = text[i + 1 : i + 3]
                i += 3
            elif i < n and text[i] == "[":
                j = text.find("]", i)
                font = text[i + 1 : j]
                i = j + 1
            else:
                font = text[i] if i < n else "R"
                i += 1
            out.append(fonts.switch(font))
        elif e == "(":
            name = text[i : i + 2]
            i += 2
            out.append(esc(CHARS.get(name, "")))
        elif e == "[":
            j = text.find("]", i)
            name = text[i:j]
            i = j + 1
            out.append(esc(CHARS.get(name, "")))
        elif e == "*":
            # a string: \*(rg, \*X
            if i < n and text[i] == "(":
                name = text[i + 1 : i + 3]
                i += 3
            else:
                name = text[i : i + 1]
                i += 1
            out.append(esc(STRINGS.get(name, "")))
        elif e == "s":
            # a size change: \s-1, \s0, \s+2
            m = re.match(r"[-+]?\d", text[i:])
            i += m.end() if m else 0
        elif e in "hvwlLDxNXbZoSyY":
            # \h'...' and kin: a quoted argument; a motion reads as a space
            if i < n and text[i] in "'\"":
                j = text.find(text[i], i + 1)
                i = j + 1 if j > 0 else n
            if e in "hl":
                out.append(" ")
        elif e == "n":
            # a number register: \nX, \n(XX
            i += 3 if i < n and text[i] == "(" else 1
        elif e in "&|^c%:":
            pass
        elif e == "e" or e == "\\":
            out.append("\\")
        elif e == "-":
            out.append("-")
        elif e in " 0~":
            out.append(" ")
        elif e == '"':
            break  # a comment to the end of the line
        else:
            out.append(esc(e))
    return "".join(out)


def troff_args(line):
    """A macro line's arguments, quotes honoured."""
    args = []
    for m in re.finditer(r'"((?:[^"]|"")*)"|(\S+)', line):
        args.append(m.group(1).replace('""', '"') if m.group(1) is not None else m.group(2))
    return args


def troff_book(source):
    book = Book(
        "en",
        "A Guide to the Mazes of Menace",
        "Guidebook for NetHack 5.0, original version by Eric S. Raymond, "
        "edited and expanded by Mike Stephenson and others (NetHack General Public License).",
    )
    fonts = Fonts()
    lines = source.split("\n")
    i = 0
    para = []  # BBCode pieces of the paragraph being made
    indent = 0
    numbers = [0, 0, 0, 0]
    heading = None  # the level of a heading whose title is the next line

    def flush():
        nonlocal para
        text = " ".join(p for p in para if p).strip()
        text = re.sub(r"\s+", " ", text)
        if text and plain(text).strip():
            # a font in effect goes on into the next paragraph
            text = text + fonts.close()
            if indent > 0:
                text = "[indent]" * indent + text + "[/indent]" * indent
            book.para(text)
        para = [fonts.reopen()]

    def block(rows, mono, columns=1):
        if columns > 1:
            cells = "".join(f"[cell]{c}[/cell]" for row in rows for c in row)
            book.para(f"[table={columns}]{cells}[/table]")
        else:
            body = "\n".join(r[0] if isinstance(r, list) else r for r in rows).rstrip()
            if body.strip():
                book.para(f"[code]{body}[/code]" if mono else body)

    while i < len(lines):
        line = lines[i]
        i += 1
        if heading is not None:
            flush()
            level = heading
            numbers[level - 1] += 1
            for k in range(level, len(numbers)):
                numbers[k] = 0
            number = ".".join(str(x) for x in numbers[:level])
            book.chapter(number, plain(troff_inline(line, Fonts())), level)
            heading = None
            continue
        if line.startswith(".\\\"") or line.startswith("'\\\"") or line in (".", ""):
            continue
        if line.startswith(".de "):
            while i < len(lines) and lines[i].strip() != "..":
                i += 1
            i += 1
            continue
        if line.startswith("."):
            m = re.match(r"\.\s*(\S+)\s*(.*)", line)
            if not m:
                continue
            req, rest = m.group(1), m.group(2)
            rest = rest.split('\\"')[0]
            args = troff_args(rest)
            if req == "hn":
                heading = int(args[0]) if args else 1
            elif req in ("pg", "BR", "sp", "br", "mt", "au"):
                flush()
            elif req == "ds":
                name, _, value = rest.partition(" ")
                STRINGS[name] = plain(troff_inline(value.strip(), Fonts()))
            elif req == "lp":
                flush()
                label = troff_inline(args[0], Fonts()) if args else ""
                if label.strip():
                    para.append(f"[b]{label.strip()}[/b] —")
                else:
                    para.append("")
            elif req == "op":
                word = troff_inline(args[0], Fonts()) if args else ""
                tail = troff_inline(" ".join(args[1:]), fonts) if len(args) > 1 else ""
                para.append(f"[i]{word}[/i]{tail}")
            elif req == "PS":
                flush()
                indent += 1
            elif req == "PL":
                flush()
                label = troff_inline(args[0], Fonts()) if args else ""
                para.append(f"[b]{label}[/b] —")
            elif req == "PE":
                flush()
                indent = max(0, indent - 1)
            elif req == "CC":
                flush()
                key = troff_inline(args[0], Fonts()) if args else ""
                text = troff_inline(args[1], Fonts()) if len(args) > 1 else ""
                book.para(("[indent]" * (indent + 1)) + f"[b]{key}[/b] — {text}" + ("[/indent]" * (indent + 1)))
            elif req in ("si",):
                flush()
                indent += 1
            elif req in ("ei",):
                flush()
                indent = max(0, indent - 1)
            elif req in ("sd", "SD", "nf"):
                flush()
                rows = []
                end = ("ed", "ED") if req in ("sd", "SD") else ("fi",)
                while i < len(lines):
                    l = lines[i]
                    i += 1
                    m2 = re.match(r"\.\s*(\S+)", l)
                    if m2 and m2.group(1) in end:
                        break
                    if l.startswith(".\\\"") or l == ".":
                        continue
                    if l.startswith("."):
                        if m2 and m2.group(1) == "op":
                            a = troff_args(l[3:])
                            rows.append(troff_inline(" ".join(a), Fonts()))
                        continue
                    rows.append(troff_inline(l, Fonts()))
                block(rows, True)
            elif req == "TS":
                flush()
                options = []
                spec = []
                while i < len(lines):
                    l = lines[i]
                    i += 1
                    if l.rstrip().endswith(";"):
                        options.append(l)
                        continue
                    spec.append(l)
                    if l.rstrip().endswith("."):
                        break
                tab = "\t"
                m3 = re.search(r"tab\s*\((.)\)", " ".join(options))
                if m3:
                    tab = m3.group(1)
                columns = max(len(re.findall(r"[LCRNSlcrns]", s.split(".")[0].split(",")[0].strip() or "L")) for s in spec[-1:] or ["L"])
                rows = []
                while i < len(lines):
                    l = lines[i]
                    i += 1
                    if l.startswith(".TE"):
                        break
                    if l.startswith(".\\\"") or l.startswith("."):
                        continue
                    # a line ending in \ goes on with the next
                    while l.endswith("\\") and i < len(lines):
                        l = l[:-1] + lines[i]
                        i += 1
                    cells = [troff_inline(c, Fonts()).strip() for c in l.split(tab)]
                    rows.append(cells)
                if columns > 1 and all(len(r) <= columns for r in rows):
                    rows = [r + [""] * (columns - len(r)) for r in rows if any(r)]
                    block(rows, False, columns)
                else:
                    block([[tab.join(r) if len(r) > 1 else r[0]] for r in rows], True)
            elif req == "UX":
                para.append("UNIX" + (troff_inline(args[0], fonts) if args else ""))
            elif req == "ft":
                para.append(fonts.switch(args[0] if args else "P"))
            elif req == "ce":
                flush()
            # everything else (registers, strings, conditions, indents,
            # hyphenation, page setup) does not change the text
            continue
        para.append(troff_inline(line, fonts))
    flush()
    return book


# ---- pandoc Markdown (the Russian guide) ----

def md_inline(text):
    """A paragraph's Markdown: code spans, emphasis, links to BBCode."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == "`":
            j = text.find("`", i + 1)
            if j > i:
                out.append("[code]" + esc(text[i + 1 : j]) + "[/code]")
                i = j + 1
                continue
        if c == "*" and text.startswith("**", i):
            j = text.find("**", i + 2)
            if j > i:
                out.append("[b]" + md_inline(text[i + 2 : j]) + "[/b]")
                i = j + 2
                continue
        if c in "*_" and i + 1 < n and text[i + 1] not in " \t":
            j = text.find(c, i + 1)
            # an emphasis ends where a word does
            if j > i and (j + 1 >= n or not text[j + 1].isalnum()):
                out.append("[i]" + md_inline(text[i + 1 : j]) + "[/i]")
                i = j + 1
                continue
        if c == "[":
            m = re.match(r"\[([^\]]+)\]\(([^)]+)\)", text[i:])
            if m:
                out.append(md_inline(m.group(1)))
                i += m.end()
                continue
        if c == "\\" and i + 1 < n and not text[i + 1].isalnum():
            out.append(esc(text[i + 1]))
            i += 2
            continue
        out.append(esc(c))
        i += 1
    return "".join(out)


RULE = re.compile(r"^-{3,}( +-+)*\s*$")


def md_table(lines):
    """A pandoc table from its first rule to its last: its header row (or
    None) and its rows of cells. Rows blank-line apart span several lines
    (a multiline table); else a row is a line."""
    rules = [k for k, l in enumerate(lines) if RULE.match(l)]
    gapped = next((lines[k] for k in rules if " " in lines[k].strip()), lines[rules[0]])
    starts = [m.start() for m in re.finditer(r"-+", gapped)]

    def cut(l):
        return [
            l[a : starts[k + 1] if k + 1 < len(starts) else len(l)].strip()
            for k, a in enumerate(starts)
        ]

    header = None
    body_from = rules[0] + 1
    if len(rules) >= 3 and all(lines[k].strip() for k in range(rules[0] + 1, rules[1])):
        header = [" ".join(c).strip() for c in zip(*(cut(l) for l in lines[rules[0] + 1 : rules[1]]))]
        body_from = rules[1] + 1
    body = lines[body_from : rules[-1]]
    blocks = []
    if any(not l.strip() for l in body):
        cur = []
        for l in body + [""]:
            if l.strip():
                cur.append(l)
            elif cur:
                blocks.append(cur)
                cur = []
    else:
        blocks = [[l] for l in body if l.strip()]
    rows = [[" ".join(c).strip() for c in zip(*(cut(l) for l in b))] for b in blocks]
    fmt = lambda r: [md_inline(c) for c in r]
    return (fmt(header) if header else None), [fmt(r) for r in rows]


def md_book(source):
    book = Book(
        "ru",
        "Руководство по Грозным Лабиринтам",
        "Перевод «A Guide to the Mazes of Menace» Эрика С. Рэймонда (NetHack 3.4): "
        "переводчики notabenoid.com (witmolif), оформление и правка — Вадим Великодный "
        "(github.com/velikodniy/nethack-guide). Распространяется на условиях "
        "NetHack General Public License.",
    )
    lines = source.split("\n")
    i = 0

    def number_title(text):
        m = re.match(r"^#*\s*(\d+(?:\.\d+)*)\.?\s*(.*)$", text.strip())
        number, title = (m.group(1), m.group(2)) if m else ("", text)
        return number, plain(md_inline(title.strip().rstrip(".")))

    while i < len(lines):
        line = lines[i]
        nxt = lines[i + 1] if i + 1 < len(lines) else ""
        # the title block
        if line.startswith("%"):
            i += 1
            while i < len(lines) and lines[i].startswith("  "):
                i += 1
            continue
        if not line.strip():
            i += 1
            continue
        # fenced code: lines as they are, less their common indent
        if re.match(r"^(```|~~~)", line):
            fence = line[:3]
            rows = []
            i += 1
            while i < len(lines) and not lines[i].startswith(fence):
                rows.append(lines[i].rstrip())
                i += 1
            i += 1
            pad = min((len(r) - len(r.lstrip()) for r in rows if r.strip()), default=0)
            body = "\n".join(esc(r[pad:]) for r in rows).strip("\n")
            if body.strip():
                book.para("[code]" + body + "[/code]")
            continue
        # headings: setext, ATX
        if nxt and re.match(r"^=+\s*$", nxt) and line.strip():
            number, title = number_title(line)
            book.chapter(number, title, 1)
            i += 2
            continue
        if nxt and re.match(r"^-{3,}\s*$", nxt) and line.strip() and not line.startswith("-"):
            number, title = number_title(line)
            book.chapter(number, title, 2)
            i += 2
            continue
        if line.startswith("#"):
            number, title = number_title(line.lstrip("#"))
            book.chapter(number, title, 3)
            i += 1
            continue
        # a table: from its first rule to the rule a blank line follows
        if RULE.match(line):
            block = [line]
            i += 1
            while i < len(lines):
                block.append(lines[i])
                i += 1
                if RULE.match(block[-1]) and (i >= len(lines) or not lines[i].strip()):
                    break
            header, rows = md_table(block)
            columns = max([len(r) for r in rows] + [len(header or [])] + [1])
            cells = []
            if header:
                cells += [f"[cell][b]{c}[/b][/cell]" for c in header + [""] * (columns - len(header))]
            for r in rows:
                cells += [f"[cell]{c}[/cell]" for c in r + [""] * (columns - len(r))]
            if columns > 1:
                book.para(f"[table={columns}]" + "".join(cells) + "[/table]")
            elif rows:
                book.para("[code]" + "\n".join(r[0] for r in rows) + "[/code]")
            continue
        # a horizontal rule
        if re.match(r"^(- ){3,}-?\s*$", line):
            i += 1
            continue
        # a table's caption
        if line.startswith(": "):
            text = [line[2:].strip()]
            i += 1
            while i < len(lines) and lines[i].strip() and not lines[i].startswith("-"):
                text.append(lines[i].strip())
                i += 1
            book.para("[i]" + md_inline(" ".join(text)) + "[/i]")
            continue
        # a definition: the term, then ":   " and its text (four spaces on)
        if (
            not line.startswith(" ")
            and i + 2 < len(lines)
            and not nxt.strip()
            and lines[i + 2].startswith(":")
        ):
            term = md_inline(line.strip())
            i += 2
            first = True
            while i < len(lines):
                l = lines[i]
                if l.startswith(":"):
                    text = [l[1:].strip()]
                elif l.startswith("    ") and l.strip():
                    text = [l.strip()]
                else:
                    break
                i += 1
                while i < len(lines) and lines[i].startswith("    ") and lines[i].strip():
                    text.append(lines[i].strip())
                    i += 1
                body = md_inline(" ".join(text))
                if first:
                    book.para(f"[b]{term}[/b] — {body}")
                    first = False
                else:
                    book.para(f"[indent]{body}[/indent]")
                while i < len(lines) and not lines[i].strip():
                    # a blank line ends the definition unless it goes on
                    if i + 1 < len(lines) and lines[i + 1].startswith("    ") and lines[i + 1].strip():
                        i += 1
                        break
                    i += 1
                    break
            continue
        # a list
        if re.match(r"^\s*[-*+] ", line):
            indent = len(line) - len(line.lstrip())
            text = [line.strip()[2:].strip()]
            i += 1
            while i < len(lines) and lines[i].strip() and not re.match(r"^\s*[-*+] ", lines[i]):
                text.append(lines[i].strip())
                i += 1
            pad = "[indent]" * (1 + indent // 4)
            book.para(pad + "• " + md_inline(" ".join(text)) + "[/indent]" * (1 + indent // 4))
            continue
        # code: lines four spaces in
        if line.startswith("    "):
            rows = []
            while i < len(lines) and (lines[i].startswith("    ") or not lines[i].strip()):
                if not lines[i].strip() and not (i + 1 < len(lines) and lines[i + 1].startswith("    ")):
                    break
                rows.append(esc(lines[i][4:]))
                i += 1
            book.para("[code]" + "\n".join(rows).rstrip() + "[/code]")
            continue
        # a paragraph
        text = [line.strip()]
        i += 1
        while i < len(lines) and lines[i].strip() and not re.match(r"^(=+|-{3,}( +-+)*)\s*$", lines[i]):
            if i + 1 < len(lines) and re.match(r"^(=+|-{3,})\s*$", lines[i + 1]):
                break
            text.append(lines[i].strip())
            i += 1
        book.para(md_inline(" ".join(text)))
    return book


# ---- files ----

def download(url):
    """The bytes at `url`; curl when Python has no certificates to check
    the host with (a python.org build on macOS before its Install
    Certificates)."""
    try:
        with urllib.request.urlopen(url, timeout=60) as r:
            return r.read()
    except urllib.error.URLError:
        return subprocess.run(["curl", "-fsSL", url], check=True, capture_output=True).stdout


def fetch_ru():
    lock = json.loads(LOCK.read_text())["ru"]
    data = download(lock["url"])
    got = hashlib.sha256(data).hexdigest()
    if got != lock["sha256"]:
        sys.exit(f"{lock['url']}: sha256 {got}, the lock says {lock['sha256']}")
    GUIDEBOOK_RU.write_bytes(data)
    print(f"fetched {GUIDEBOOK_RU.relative_to(ROOT)} ({len(data)} bytes, sha256 ok)")


def check_ru_source():
    lock = json.loads(LOCK.read_text())["ru"]
    got = hashlib.sha256(GUIDEBOOK_RU.read_bytes()).hexdigest()
    if got != lock["sha256"]:
        sys.exit(f"{GUIDEBOOK_RU.relative_to(ROOT)}: sha256 {got}, the lock says {lock['sha256']}")


def outputs():
    check_ru_source()
    return {
        HELP / "guidebook.en.json": troff_book(GUIDEBOOK_MN.read_text(encoding="utf-8")).json(),
        HELP / "guidebook.ru.json": md_book(GUIDEBOOK_RU.read_text(encoding="utf-8")).json(),
    }


def dump(book):
    return json.dumps(book, ensure_ascii=False, indent=0) + "\n"


def main(argv):
    if "--fetch" in argv:
        fetch_ru()
    stale = []
    for path, book in outputs().items():
        text = dump(book)
        if "--check" in argv:
            if not path.exists() or path.read_text(encoding="utf-8") != text:
                stale.append(str(path.relative_to(ROOT)))
            continue
        path.write_text(text, encoding="utf-8")
        n = sum(len(c["paragraphs"]) for c in book["chapters"])
        print(f"{path.relative_to(ROOT)}: {len(book['chapters'])} chapters, {n} paragraphs")
    if stale:
        sys.exit("out of date (run tools/help/guidebook.py): " + ", ".join(stale))


if __name__ == "__main__":
    main(sys.argv[1:])
