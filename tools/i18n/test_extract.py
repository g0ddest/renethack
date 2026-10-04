"""Tests of the extractor: python3 -m unittest discover -s tools/i18n"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import clex  # noqa: E402
import datfiles  # noqa: E402
import extract  # noqa: E402


def catalog_of(src, headers=""):
    """The catalog of one C file (and string macros of `headers`)."""
    glob = extract.Globals()
    glob.strings.update(clex.scan_unit("h.h", headers).strings)
    unit = clex.scan_unit("t.c", src)
    contexts = [extract.Context(glob, unit, f) for f in unit.functions]
    for ctx in contexts:
        glob.index(ctx)
    cat = extract.Catalog()
    for ctx in contexts:
        extract.scan_function(cat, ctx)
    return cat.entries


class Lexer(unittest.TestCase):
    def test_strings_escapes_and_if0(self):
        toks = clex.lex('x = "a\\tb\\"c\\033"; /* "no" */\n#if 0\ny = "dead";\n#else\nz = \'\\n\';\n#endif\n')
        strings = [t.value for t in toks if t.kind in ("string", "char")]
        self.assertEqual(strings, ['a\tb"c\x1b', "\n"])

    def test_functions_and_lines(self):
        unit = clex.scan_unit("t.c", "static int t[] = { 1 };\n\nint\nf(int a)\n{\n    return a;\n}\n")
        self.assertEqual([(f.name, f.line) for f in unit.functions], [("f", 4)])


class Formats(unittest.TestCase):
    def test_prefixes_and_kinds(self):
        e = catalog_of('void f(struct monst *m) { You("hit %s.", mon_nam(m)); You_feel("%s.", "ill"); }')
        self.assertEqual(e["You hit %s."].args, [{"monster"}])
        self.assertIn("You feel ill.", e)
        self.assertIn("You dream that you feel ill.", e)
        self.assertIn("src/t.c:1 f You(", e["You hit %s."].sites[0])

    def test_literal_arguments_give_derived_entries(self):
        e = catalog_of('void f(struct monst *m, int p) {\n'
                       '    You("%s %s.", p ? "swap places with" : "frighten", mon_nam(m));\n}\n')
        self.assertIn("You swap places with %s.", e)
        self.assertIn("You frighten %s.", e)
        self.assertEqual(e["You frighten %s."].bases, ["You %s %s."])
        self.assertTrue(e["You %s %s."].expanded)

    def test_variables_switches_and_verbs(self):
        e = catalog_of('void f(struct monst *m, int t) {\n'
                       '    const char *verb = 0;\n'
                       '    switch (t) { case 1: verb = "bites"; break; default: verb = "hits"; }\n'
                       '    pline("%s %s!", Monnam(m), verb);\n'
                       '    pline("%s %s.", Tobjnam(o, "glow"), vtense(0, "be"));\n}\n')
        self.assertIn("%s bites!", e)
        self.assertIn("%s hits!", e)
        self.assertIn("%s glows bes.", e)
        self.assertEqual(e["%s glow be."].args, [{"object"}])

    def test_buffers_are_inlined(self):
        e = catalog_of('void f(struct monst *m) {\n'
                       '    char buf[BUFSZ];\n'
                       '    Sprintf(buf, "%s bites", Monnam(m));\n'
                       '    if (m->mtame)\n'
                       '        Strcat(buf, " hard");\n'
                       '    pline("%s %s.", buf, mon_nam(m));\n'
                       '    Strcpy(buf, "Other text");\n'
                       '    pline("%s!", buf);\n}\n')
        self.assertIn("%s bites %s.", e)
        self.assertIn("%s bites hard %s.", e)
        self.assertIn("Other text!", e)
        # the second use only sees what was written after the first
        self.assertNotIn("Other text %s.", e)
        self.assertNotIn("%s bites!", e)

    def test_appends_that_always_run(self):
        e = catalog_of('void f(int a) {\n'
                       '    char buf[BUFSZ];\n'
                       '    Sprintf(buf, " who %s opposed by", a ? "is" : "was");\n'
                       '    if (a != 1)\n'
                       '        Sprintf(eos(buf), " %s (%s) and", g(1), s(1));\n'
                       '    if (a != 2) {\n'
                       '        Sprintf(eos(buf), " %s (%s)", g(2), s(2));\n'
                       '    }\n'
                       '    Strcat(buf, ".");\n'
                       '    pline("%s", buf);\n}\n')
        self.assertIn(" who is opposed by %s (%s) and %s (%s).", e)
        self.assertIn(" who was opposed by %s (%s).", e)
        # the last append always runs: no text ends before it
        self.assertNotIn("pline", e[" who is opposed by"].uses)

    def test_what_is_appended_to_a_call_s_text(self):
        e = catalog_of('static void value(int n, char out[]) { out[0] = 0; }\n'
                       'void f(int a, int b) {\n'
                       '    char buf[BUFSZ];\n'
                       '    value(a, buf);\n'
                       '    if (a != b)\n'
                       '        Sprintf(eos(buf), " (current; limit:%d", b);\n'
                       '    if (a)\n'
                       '        Strcat(buf, ")");\n'
                       '    pline("Your strength is %s.", buf);\n}\n')
        self.assertIn("Your strength is %s (current; limit:%d).", e)
        self.assertIn("Your strength is %s.", e)

    def test_a_call_may_write_a_buffer(self):
        src = ('static void fill(char *out) { out[0] = 0; }\n'
               'static void show(const char *s) { (void) s; }\n'
               'void f(void) {\n'
               '    char buf[BUFSZ];\n'
               '    Strcpy(buf, "First text");\n'
               '    CALL(buf);\n'
               '    Strcat(buf, " more");\n'
               '    pline("%s!", buf);\n}\n')
        self.assertIn("First text more!", catalog_of(src.replace("CALL", "show")))
        self.assertNotIn("First text more!", catalog_of(src.replace("CALL", "fill")))

    def test_a_format_built_in_a_buffer(self):
        e = catalog_of('void f(struct monst *m, int p) {\n'
                       '    char fmtbuf[BUFSZ];\n'
                       '    Snprintf(fmtbuf, sizeof fmtbuf, "%s %s is %%s!", p ? "That" : "This", "thing");\n'
                       '    pline(fmtbuf, a_monnam(m));\n}\n')
        self.assertIn("That thing is %s!", e)
        self.assertEqual(e["That thing is %s!"].args, [{"monster"}])

    def test_questions_built_by_helpers(self):
        e = catalog_of('static void ask(const char *prompt) { getlin(prompt, NULL); }\n'
                       'void f(struct obj *o) {\n'
                       '    char qbuf[QBUFSZ];\n'
                       '    (void) safe_qbuf(qbuf, "Call ", ":", o, xname, simpleonames, "thing");\n'
                       '    ask(qbuf);\n'
                       '    Sprintf(qbuf, "There %s ", otense(o, "are"));\n'
                       '    (void) safe_qbuf(qbuf, qbuf, " here; eat it?", o, doname, xname, "it");\n'
                       '    (void) yn_function(qbuf, ynchars, 0, TRUE);\n}\n')
        self.assertEqual(e["Call %s:"].args, [{"object"}])
        self.assertIn("There is %s here; eat it?", e)

    def test_tables_of_rows(self):
        e = catalog_of('static const char *const orders[2][2] = { {"a", "alphabetically"}, {"n", "by count"} };\n'
                       'void f(int i) { add_menu(w, g, &a, 0, 0, 0, 0, orders[i][1], 0); }\n')
        self.assertIn("alphabetically", e)
        self.assertIn("by count", e)
        self.assertNotIn("a", e)

    def test_helpers_returns_and_parameters(self):
        e = catalog_of('static const char *exclam(int d) { return d > 4 ? "!" : "."; }\n'
                       'static void hit(const char *what, int d) { You("hit %s%s", what, exclam(d)); }\n'
                       'void g(void) { hit("the wall", 1); hit("the door", 9); }\n')
        self.assertIn("You hit the wall!", e)
        self.assertIn("You hit the door.", e)

    def test_macros_and_common_strings(self):
        e = catalog_of('void f(void) { pline1(GREETING); pline(silly, "eat"); }',
                       '#define GREETING "Hello" " there."\n#define silly "That is silly to %s."\n')
        self.assertIn("Hello there.", e)
        self.assertIn("That is silly to eat.", e)

    def test_punctuation_alone_is_left_out(self):
        e = catalog_of('void f(char c, struct obj *o) { pline("%s.", xname(o)); pline("%c - %s.", c, doname(o)); }')
        self.assertNotIn("%s.", e)
        self.assertIn("%c - %s.", e)

    def test_verb_forms(self):
        forms = [extract.verb_s(v) for v in ("hit", "kiss", "fly", "are", "have", "go", "watch")]
        self.assertEqual(forms, ["hits", "kisses", "flies", "is", "has", "goes", "watches"])


class Data(unittest.TestCase):
    def test_quest_codes_become_placeholders(self):
        fmt, kinds = datfiles.quest_format("%pC, %lh says 100%% that %ds will %x %oA.")
        self.assertEqual(fmt, "%s, %s says 100%% that %s will %s %s.")
        self.assertEqual(kinds, ["quest:pC", "quest:lh", "quest:ds", "quest:x", "quest:oA"])
        # a pronoun suffix only follows %d %l %n %o
        fmt, kinds = datfiles.quest_format("%ph")
        self.assertEqual((fmt, kinds), ("%sh", ["quest:p"]))

    def test_lua_concatenations(self):
        toks = datfiles.lua_lex('des.engraving({ text = "Use \'" .. key("up") .. "\' to go up" })')
        args = datfiles.lua_args(toks, 3)
        parts = datfiles.concat_parts(datfiles.engraving_texts(args)[0])
        self.assertEqual(parts, ["Use '", None, "' to go up"])


if __name__ == "__main__":
    unittest.main()
