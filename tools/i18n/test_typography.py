"""Tests of the translations' typography: python3 -m unittest discover -s tools/i18n"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import typography  # noqa: E402


class Fix(unittest.TestCase):
    def test_one_space_after_a_sentence(self):
        self.assertEqual(
            typography.fix("Хрясь!  Вы выбиваете потайную дверь!"),
            "Хрясь! Вы выбиваете потайную дверь!",
        )
        # after a closing quote, a placeholder that renders as nothing, a colon
        self.assertEqual(
            typography.fix("[{1:cap} хвастает.{2:skip}  «Готовься!»]"),
            "[{1:cap} хвастает.{2:skip} «Готовься!»]",
        )
        self.assertEqual(typography.fix("Мелодия звучит так:  «{1}»."), "Мелодия звучит так: «{1}».")
        # an indent, a run of three, a gap after a word: layout, kept
        for kept in ["гремит голос:\n\n    «Ты", "Конец.   Дальше", "{1}  {2}"]:
            self.assertEqual(typography.fix(kept), kept)

    def test_the_full_stop_after_the_guillemet(self):
        self.assertEqual(typography.fix("«Ты пришёл слишком поздно.»"), "«Ты пришёл слишком поздно».")
        self.assertEqual(typography.fix("«Иди, {1}.»]"), "«Иди, {1}».]")
        # ! ? and an ellipsis stay inside
        for kept in ["«Готовься к смерти!»", "«Кто здесь?»", "«Ну...»", "«Ну…»"]:
            self.assertEqual(typography.fix(kept), kept)

    def test_quotes_are_guillemets(self):
        self.assertEqual(typography.fix('Волшебное слово — "XYZZY".'), "Волшебное слово — «XYZZY».")
        self.assertEqual(typography.fix("Говорят, `клин клином' лечит."), "Говорят, «клин клином» лечит.")
        self.assertEqual(typography.fix("Говорят, 'Elbereth' пишут."), "Говорят, «Elbereth» пишут.")
        self.assertEqual(typography.fix('«Он сказал: "стой".»'), "«Он сказал: „стой“».")
        # keys stay as they are, and so does an empty pair
        for kept in ["клавишей '{1}'", "буква 'c', а не 'e'", "префикс 'm'", 'Выйти из поиска ("")']:
            self.assertEqual(typography.fix(kept), kept)

    def test_layout_is_no_prose(self):
        self.assertFalse(typography.prose({"uses": ["window"], "fmt": "You are %s."}))
        self.assertFalse(typography.prose({"uses": ["menu"], "fmt": "%-20s  %2d   attack"}))
        self.assertFalse(typography.prose({"uses": ["pline"], "fmt": "Status of %s:  Level %d  HP %d"}))
        self.assertFalse(typography.prose({"uses": ["sprintf"], "fmt": "%s        a trap"}))
        self.assertTrue(typography.prose({"uses": ["pline"], "fmt": "Crash!  You kick open a secret door!"}))
        self.assertTrue(typography.prose({"uses": ["quest"], "fmt": "gasps:\n\n    \"Thou hast won.\""}))
        self.assertTrue(typography.prose({"uses": ["rumor"], "fmt": "marked `Caution:  Trap Door'."}))


class Translations(unittest.TestCase):
    def test_every_translation_is_typeset(self):
        changes = typography.run(write=False)
        self.assertEqual(
            [],
            [f"{name} [{id_}]: {new}" for name, id_, _, new in changes[:10]],
            f"{len(changes)} translations need typography: run python3 tools/i18n/typography.py",
        )


if __name__ == "__main__":
    unittest.main()
