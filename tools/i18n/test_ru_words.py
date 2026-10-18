"""Every word a choice by gender gives in client/i18n/ru/*.toml is a word:
a heroine never reads «нашёлла» (a feminine ending glued to a masculine
word). Needs pymorphy3 (tools/i18n/requirements.txt); skipped without it.

    python3 -m unittest tools/i18n/test_ru_words.py
"""
import glob
import os
import re
import tomllib
import unittest

try:
    import pymorphy3
except ImportError:  # the extractor's tests run without it
    pymorphy3 = None

RU_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "client", "i18n", "ru")
# {hero:gender|…|…}, {2:gender|…|…|…|…}
CHOICE = re.compile(r"\{(?:hero|\d+):gender((?:\|[^{}|]*)+)\}")
WORD = re.compile(r"[А-Яа-яЁё]+(?:-[А-Яа-яЁё]+)*")
# words a dictionary does not know: names, slang a hallucinating hero
# hears, the words of the game
KNOWN = {
    "харона", "сестринство", "эльфийка", "нудисткой",
    "телепортировался", "телепортировалась",
    "отпадный", "отпадная", "отпадное", "отпадные",
    "вкуснющий", "вкуснющая", "вкуснющее", "вкуснющие",
    "гр-р-рандиозный", "гр-р-рандиозная", "гр-р-рандиозное", "гр-р-рандиозные",
}


def choices():
    """(file, key, branch) of every choice by gender."""
    for path in sorted(glob.glob(os.path.join(RU_DIR, "*.toml"))):
        with open(path, "rb") as f:
            table = tomllib.load(f)
        for key, entry in table.items():
            texts = [entry.get("ru", "")] + list(entry.get("forms", []) or [])
            for text in texts:
                for m in CHOICE.finditer(text):
                    for branch in m.group(1).split("|")[1:]:
                        yield os.path.basename(path), key, branch


@unittest.skipIf(pymorphy3 is None, "pymorphy3 is not installed")
class TestGenderedWords(unittest.TestCase):
    def test_every_gendered_word_is_a_word(self):
        morph = pymorphy3.MorphAnalyzer()
        bad = []
        for path, key, branch in choices():
            for word in WORD.findall(branch):
                w = word.lower()
                if w in KNOWN or all(morph.word_is_known(p) for p in w.split("-")):
                    continue
                bad.append(f"{path} [{key}]: {word!r} in {branch!r}")
        self.assertEqual(bad, [], "\n".join(bad))


if __name__ == "__main__":
    unittest.main()
