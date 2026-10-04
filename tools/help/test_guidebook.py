#!/usr/bin/env python3
"""Tests of tools/help/guidebook.py: the troff and the Markdown each made
into chapters of BBCode paragraphs.

    python3 tools/help/test_guidebook.py
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import guidebook  # noqa: E402


def chapters(book):
    return [(c["number"], c["title"], c["level"], c["paragraphs"]) for c in book.json()["chapters"]]


class Troff(unittest.TestCase):
    def test_headings_number_themselves(self):
        book = guidebook.troff_book(
            ".hn 1\nIntroduction\n.pg\nRecently,\nyou began.\n.hn 2\nThe map\n.pg\nIt shows.\n"
        )
        got = chapters(book)
        self.assertEqual([(n, t, l) for n, t, l, _ in got], [("1", "Introduction", 1), ("1.1", "The map", 2)])
        self.assertEqual(got[0][3], ["Recently, you began."])

    def test_fonts_and_characters(self):
        book = guidebook.troff_book(
            ".hn 1\nX\n.pg\nThe \\fBbold\\fP and \\fIitalic\\fP, \\(lqquoted\\(rq, \\f(CR[x]\\fP\\(em done.\n"
        )
        para = chapters(book)[0][3][0]
        self.assertEqual(para, "The [b]bold[/b] and [i]italic[/i], “quoted”, [code][lb]x][/code]— done.")

    def test_labelled_paragraphs_and_options(self):
        book = guidebook.troff_book(
            '.hn 1\nX\n.lp "Gold    "\nThe gold you carry.\n.lp ""\nMore of it.\n.pg\nSee the\n.op time\noption.\n'
        )
        self.assertEqual(
            chapters(book)[0][3],
            ["[b]Gold[/b] — The gold you carry.", "More of it.", "See the [i]time[/i] option."],
        )

    def test_displays_keep_their_lines(self):
        book = guidebook.troff_book(".hn 1\nX\n.sd\nline one\n  line two\n.ed\n")
        self.assertEqual(chapters(book)[0][3], ["[code]line one\n  line two[/code]"])

    def test_tables_of_columns(self):
        book = guidebook.troff_book(".hn 1\nX\n.TS\nbox center;\nC C.\na\tb\nc\td\n.TE\n")
        self.assertEqual(
            chapters(book)[0][3],
            ["[table=2][cell]a[/cell][cell]b[/cell][cell]c[/cell][cell]d[/cell][/table]"],
        )


class Markdown(unittest.TestCase):
    def test_headings_of_both_kinds(self):
        book = guidebook.md_book(
            "1. Вступление\n=============\n\nТекст.\n\n3.1. Строка `состояния`\n------------\n\nЕщё.\n\n###5.5.1. Особенности\n\nИ ещё.\n"
        )
        got = chapters(book)
        self.assertEqual(
            [(n, t, l) for n, t, l, _ in got],
            [("1", "Вступление", 1), ("3.1", "Строка состояния", 2), ("5.5.1", "Особенности", 3)],
        )

    def test_inline_markup(self):
        book = guidebook.md_book("1. X\n====\n\nКоманда «`[`» и *курсив*, [ссылка](http://x).\n")
        self.assertEqual(chapters(book)[0][3], ["Команда «[code][lb][/code]» и [i]курсив[/i], ссылка."])

    def test_definitions(self):
        book = guidebook.md_book("1. X\n====\n\nВарвары (Barbarians)\n\n:   Это воины,\n    любят битвы.\n")
        self.assertEqual(chapters(book)[0][3], ["[b]Варвары (Barbarians)[/b] — Это воины, любят битвы."])

    def test_a_multiline_table(self):
        text = (
            "1. X\n====\n\n"
            "------- ----------------\n"
            "Символ  Значение\n"
            "------- ----------------\n"
            "`-` и   Стены комнаты\n"
            "`|`     или дверь.\n"
            "\n"
            "`.`     Пол.\n"
            "------- ----------------\n\n"
        )
        para = chapters(guidebook.md_book(text))[0][3][0]
        self.assertEqual(
            para,
            "[table=2][cell][b]Символ[/b][/cell][cell][b]Значение[/b][/cell]"
            "[cell][code]-[/code] и [code]|[/code][/cell][cell]Стены комнаты или дверь.[/cell]"
            "[cell][code].[/code][/cell][cell]Пол.[/cell][/table]",
        )

    def test_fenced_code(self):
        book = guidebook.md_book("1. X\n====\n\n```\n    The bat bites!\n    |....|\n```\n")
        self.assertEqual(chapters(book)[0][3], ["[code]The bat bites!\n|....|[/code]"])


class Plain(unittest.TestCase):
    def test_tags_go_for_a_search(self):
        self.assertEqual(guidebook.plain("[b]a[/b] [code][lb]x][/code]"), "a [x]")


if __name__ == "__main__":
    unittest.main()
