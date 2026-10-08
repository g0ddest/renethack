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

    def test_a_member_its_unit_sets(self):
        e = catalog_of('static void verb(struct ctx *de) {\n'
                       '    de->everb = 0;\n'
                       '    de->everb = de->adding ? "add to the writing in" : "write in";\n'
                       '    de->eloc = "dust";\n}\n'
                       'void f(struct ctx *de) {\n'
                       '    de->eloc = surface(u.ux, u.uy);\n'
                       '    verb(de);\n'
                       '    You("%s the %s.", de->everb, de->eloc);\n}\n')
        self.assertIn("You write in the %s.", e)
        self.assertIn("You add to the writing in the %s.", e)
        # set to something else too: no literal of its own
        self.assertNotIn("You write in the dust.", e)

    def test_one_text_for_another_in_a_buffer(self):
        e = catalog_of('static const char *const texts[] = {\n'
                       '    "You are slowing down.", "Your limbs are stiffening."\n};\n'
                       'void f(int i) {\n    char buf[BUFSZ];\n'
                       '    Strcpy(buf, texts[i]);\n'
                       '    if (nolimbs(d) && strstri(buf, "limbs"))\n'
                       '        (void) strsubst(buf, "limbs", "extremities");\n'
                       '    urgent_pline("%s", buf);\n}\n')
        self.assertEqual(e["Your limbs are stiffening."].uses, {"pline", "sprintf"})
        self.assertEqual(e["Your extremities are stiffening."].uses, {"pline"})
        self.assertIn("You are slowing down.", e)

    def test_the_sentences_of_an_argument_too_varied_to_derive(self):
        voices = ", ".join(f'"v{i}"' for i in range(9))
        calls = "".join(f'void c{i}(void) {{ say("Thou art number {i}."); }}\n' for i in range(12))
        e = catalog_of(f'static const char *const voices[] = {{ {voices} }};\n'
                       'static void say(const char *words) {\n'
                       '    pline("A voice %s: %s", voices[rn2(9)], words);\n}\n' + calls)
        # 9 voices by 12 sentences: the voices are put in, the sentences are pieces
        self.assertEqual(e["A voice v3: %s"].uses, {"pline"})
        self.assertEqual(e["Thou art number 7."].uses, {"sprintf"})
        self.assertNotIn("A voice v3: Thou art number 7.", e)

    def test_a_text_from_its_third_character_and_one_of_a_table(self):
        e = catalog_of('static const char *action(int lock) {\n'
                       '    static const char *const actions[] = { "unlocking the door", "picking the lock" };\n'
                       '    if (lock) return actions[0] + 2;\n'
                       '    return pick ? actions[1] : actions[0];\n}\n'
                       'void f(void) { You("succeed in %s.", action(x)); }\n')
        self.assertEqual({k for k in e if k.startswith("You succeed") and "%" not in k},
                         {"You succeed in locking the door.", "You succeed in unlocking the door.",
                          "You succeed in picking the lock."})

    def test_the_columns_an_index_chooses_between(self):
        e = catalog_of('static const char *const exertext[3][2] = {\n'
                       '    { "exercising diligently", "exercising properly" }, { 0, 0 },\n'
                       '    { "very observant", "paying attention" } };\n'
                       'void f(int i, int mod) {\n'
                       '    You("%s %s.", (mod > 0) ? "must have been" : "haven\'t been",\n'
                       '        exertext[i][(mod > 0) ? 0 : 1]);\n}\n')
        self.assertIn("You must have been very observant.", e)
        self.assertIn("You haven't been paying attention.", e)

    def test_a_format_and_a_word_handed_on(self):
        e = catalog_of('static void flow(const char *fillmsg) { pline(fillmsg, hliquid("water")); }\n'
                       'void a(void) { flow("The hole fills with %s!"); }\n'
                       'void b(const char *m) { flow(m); }\n'
                       'static void silly_thing(const char *word) { pline("That is a silly thing to %s.", word); }\n'
                       'static void getobj(const char *word) { silly_thing(word); }\n'
                       'void c(void) { getobj("eat"); }\n'
                       'void d(char *w) { getobj(w); }\n')
        self.assertIn("The hole fills with %s!", e)
        self.assertIn("That is a silly thing to eat.", e)

    def test_an_article_a_null_arm_and_an_ing_form(self):
        for verb, ing in [("bash", "bashing"), ("strike", "striking"), ("tip", "tipping"),
                          ("slither", "slithering"), ("vie", "vying"), ("put on", "putting on")]:
            self.assertEqual(extract.ing_suffix(verb), ing)
        for text, articled in [("apron", "an apron"), ("helm", "a helm"), ("unicorn horn", "a unicorn horn"),
                               ("iron bars", "iron bars"), ("x", "an x"), ("one-eyed orc", "a one-eyed orc")]:
            self.assertEqual(extract.an(text), articled)
        e = catalog_of('static const char c_shield[] = "shield", c_suit[] = "suit", c_cloak[] = "cloak";\n'
                       'static void already(const char *cc) { You("are already wearing %s.", cc); }\n'
                       'void f(struct obj *o) {\n'
                       '    const char *which = is_cloak(o) ? c_cloak : is_suit(o) ? c_suit : 0;\n'
                       '    already(an(c_shield));\n'
                       '    if (which) pline_The("%s will not fit.", which);\n'
                       '    You("begin %s monsters.", ing_suffix(monk ? "strike" : "bash"));\n}\n')
        self.assertIn("You are already wearing a shield.", e)
        self.assertIn("The suit will not fit.", e)
        self.assertIn("The cloak will not fit.", e)
        self.assertIn("You begin striking monsters.", e)

    def test_emptied_in_another_arm(self):
        e = catalog_of('void f(struct obj *w) {\n    char buf[BUFSZ];\n    *buf = \'\\0\';\n'
                       '    if (w->oeroded) Sprintf(buf, " and %s now as good as new", otense(w, "are"));\n'
                       '    if (w->cursed) {\n        pline("%s amber%s.", Yobjnam2(w, "glow"), buf);\n'
                       '        *buf = \'\\0\';\n    } else if (!w->blessed) {\n'
                       '        pline("%s with an aura%s.", Yobjnam2(w, "glow"), buf);\n    }\n}\n')
        self.assertIn("%s glows with an aura and is now as good as new.", e)
        self.assertIn("%s glows with an aura.", e)

    def test_how_a_creature_staggers_and_a_text_copied_into_a_member(self):
        e = catalog_of('static const char *const levitate[4] = { "float", "Float", "wobble", "Wobble" },\n'
                       '    *const crawl[4] = { "crawl", "Crawl", "lurch", "Lurch" };\n'
                       'const char *stagger(const struct permonst *ptr, const char *def) {\n'
                       '    int locoindx = (*def != highc(*def)) ? 2 : 3;\n'
                       '    return (is_floater(ptr) ? levitate[locoindx] : nolimbs(ptr) ? crawl[locoindx] : def);\n}\n'
                       'void f(struct monst *m) {\n'
                       '    pline("%s %s for a moment.", Monnam(m), makeplural(stagger(m->data, "stagger")));\n'
                       '    (void) strncpy(svc.context.takeoff.disrobing, (mask ? "disrobing" : "disarming"), 10);\n'
                       '    You("finish %s.", svc.context.takeoff.disrobing);\n}\n')
        self.assertEqual({k for k in e if k.endswith("for a moment.") and k.count("%") == 1},
                         {"%s wobbles for a moment.", "%s lurches for a moment.", "%s staggers for a moment."})
        self.assertIn("You finish disarming.", e)

    def test_a_format_a_variable_holds_and_a_creature_doing_it_to_itself(self):
        e = catalog_of('void f(struct monst *m) {\n'
                       '    static const char *const fam[3] = { "You have a sense of deja vu.",\n'
                       '        "This place %s familiar...", 0 };\n'
                       '    const char *mesg = fam[rn2(3)];\n    char buf[BUFSZ];\n'
                       '    if (mesg && strchr(mesg, \'%\')) {\n'
                       '        Sprintf(buf, mesg, !Blind ? "looks" : "seems");\n        mesg = buf;\n    }\n'
                       '    if (mesg) pline1(mesg);\n'
                       '    pline("%s with %s!", monverbself(m, Monnam(m), "zap", (char *) 0), doname(o));\n}\n')
        self.assertIn("This place looks familiar...", e)
        self.assertIn("This place seems familiar...", e)
        self.assertEqual(e["%s zaps herself with %s!"].args, [{"monster"}, {"object"}])
        self.assertIn("%s zap themselves with %s!", e)

    def test_a_name_goes_before_a_verb(self):
        callers = "".join(f'void c{i}(struct obj *o) {{ erode_obj(o, "{w}", {i}); }}\n'
                          for i, w in enumerate(["gloves", "boots", "cloak"]))
        e = catalog_of('int erode_obj(struct obj *o, const char *ostr, int type) {\n'
                       '    static const char *const action[] = { "smoulder", "rust", "rot", "corrode", "crack" };\n'
                       '    const char *adverb = full ? " completely" : some ? " further" : "";\n'
                       '    pline("%s %s %s%s!", uvictim ? "Your" : !vismon ? "The" : s_suffix(Monnam(victim)),\n'
                       '          ostr, vtense(ostr, action[type]), adverb);\n    return 0;\n}\n'
                       'void d(struct obj *o) { erode_obj(o, xname(o), 1); }\n' + callers)
        self.assertIn("Your %s rusts further!", e)
        self.assertIn("The %s crack completely!", e)
        self.assertNotIn("Your gloves %s further!", e)

    def test_what_a_poisoned_hero_is_told(self):
        unit = clex.scan_unit("attrib.c", (
            'static const struct poison_effect_message {\n    void (*delivery_func)(const char *, ...);\n'
            '    const char *effect_msg;\n} poiseff[] = {\n    { You_feel, "weaker" },\n'
            '    { Your, "brain is on fire" },\n};\n'
            'void poisontell(int typ, boolean exclaim) {\n'
            '    void (*func)(const char *, ...) = poiseff[typ].delivery_func;\n'
            '    const char *msg_txt = poiseff[typ].effect_msg;\n'
            '    if (typ == A_STR && ACURR(A_STR) == STR19(25))\n        msg_txt = "innately weaker";\n'
            '    (*func)("%s%c", msg_txt, exclaim ? \'!\' : \'.\');\n}\n'))
        cat = extract.Catalog()
        extract.add_poiseff(cat, [unit])
        self.assertEqual({k for k in cat.entries if not k.startswith("You dream")},
                         {"You feel weaker.", "You feel weaker!", "Your brain is on fire.", "Your brain is on fire!",
                          "You feel innately weaker.", "You feel innately weaker!"})

    def test_a_column_of_a_few_words(self):
        e = catalog_of('static const char *school(int s) { switch (s) { case 1: return "attack"; default: return ""; } }\n'
                       'void dospellmenu(void) {\n    const char *fmt = tabs ? "%s\\t%s" : "%-20s  %2d   %-12s %3d%%";\n'
                       '    Sprintf(buf, fmt, spellname(i), spellev(i), school(i), fail(i));\n'
                       '    add_menu(w, g, &a, 0, 0, 0, 0, buf, 0);\n}\n')
        self.assertIn("%-20s  %2d   attack       %3d%%", e)
        self.assertNotIn("%-20s  %2d   %-12s %3d%%", [k for k in e if e[k].uses == {"menu"}])

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

    def test_what_a_helper_writes_or_appends(self):
        src = ('static char *trap_predicament(char *outbuf) {\n'
               '    Strcpy(outbuf, "trapped");\n'
               '    Strcat(outbuf, " in a pit");\n'
               '    return outbuf;\n}\n'
               'static void append_honorific(char *buf) { Strcat(buf, "esteemed sir"); }\n'
               'void f(int angry) {\n'
               '    char p[BUFSZ], buf[BUFSZ];\n'
               '    (void) trap_predicament(p);\n'
               '    pline("You are %s.", p);\n'
               '    Strcpy(buf, "For you,");\n'
               '    if (angry) Strcat(buf, " scum;");\n'
               '    else { Strcat(buf, " "); append_honorific(buf); Strcat(buf, "; only"); }\n'
               '    pline("%s %d zorkmids.", buf, 5);\n}\n')
        e = catalog_of(src)
        self.assertIn("You are trapped in a pit.", e)
        self.assertIn("For you, %s; only %d zorkmids.", e)
        self.assertIn("For you, scum; %d zorkmids.", e)

    def test_one_of_an_array(self):
        e = catalog_of('static const char *const sounds[] = { "beep", "boing" };\n'
                       'void f(struct monst *m) {\n'
                       '    const char *verb = ROLL_FROM(sounds);\n'
                       '    pline("%s %s!", Monnam(m), vtense((char *) 0, verb));\n}\n')
        self.assertIn("%s beeps!", e)
        self.assertIn("%s boings!", e)

    def test_a_call_s_text_and_a_write_in_a_branch(self):
        # insight.c: "You have been killed 3 times" (N_times writes it) or
        # "You are dead (2nd time!)", or "You are dead"
        src = ('static const char You_[] = "You ";\n'
               'static void count(long n, char *out) { out[0] = 0; }\n'
               'void f(int final, long n) {\n'
               '    char buf[BUFSZ];\n'
               '    buf[0] = 0;\n'
               '    if (final < 2)\n'
               '        count(n, buf);\n'
               '    else if (n > 1)\n'
               '        Sprintf(buf, " (%ld times!)", n);\n'
               '    enl_msg(You_, "have been killed ", "are dead", buf, "");\n}\n')
        e = catalog_of(src)
        self.assertIn(" You are dead (%ld times!).", e)
        self.assertIn(" You have been killed %s.", e)
        # one after the other: the write is what shows
        e = catalog_of(src.replace("if (final < 2)", "").replace("else if (n > 1)", ""))
        self.assertIn(" You are dead (%ld times!).", e)
        self.assertNotIn(" You have been killed %s.", e)

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

    def test_the_extended_commands(self):
        unit = clex.scan_unit("cmd.c", (
            'struct ext_func_tab extcmdlist[] = {\n'
            '    { \'#\', "#", "enter and perform an extended command", doextcmd, 0, NULL },\n'
            '    { M(\'a\'), "adjust", "adjust inventory letters", doorganize, 0, NULL },\n'
            '    { 0, "genocided", "list monsters that have been genocided or become extinct",\n'
            '      dogenocided, 0, NULL },\n'
            '    { 0, (char *) 0, (char *) 0, donull, 0, (char *) 0 }\n'
            '};\n'
            'int doextlist(void) { Sprintf(buf, " %-14s %4s %s", efp->ef_txt, fl, desc); }\n'))
        cat = extract.Catalog()
        extract.add_extcmds(cat, [unit])
        e = cat.entries
        self.assertIn("adjust inventory letters", e)
        self.assertEqual(e[" adjust         %4s %s"].sites, ["src/cmd.c:8 doextlist adjust"])
        self.assertIn(" #              %4s enter and perform an extended command", e)
        self.assertIn("list monsters that have been genocided", e)
        self.assertEqual(len(e), 7)

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

    def test_a_local_of_formats(self):
        # themerms.lua: the engraving that points at a buried treasure
        src = """
            local dig = "";
            if (tx == 0 and ty == 0) then
               dig = " here";
            else
               if (tx ~= 0) then
                  dig = string.format(" %i %s", math.abs(tx), (tx > 0) and "east" or "west");
               end
               if (ty ~= 0) then
                  dig = dig .. string.format(" %i %s", math.abs(ty), (ty > 0) and "south" or "north");
               end
            end
            des.engraving({ coord = pos, type = "burn", text = "Dig" .. dig });
        """
        toks = datfiles.lua_lex(src)
        at = next(i for i, t in enumerate(toks) if t.text == "engraving")
        expr = datfiles.engraving_texts(datfiles.lua_args(toks, at + 1))[0]
        texts = dict(datfiles.local_texts(toks, expr, at))
        self.assertEqual(texts["Dig here"], [])
        self.assertEqual(texts["Dig %d %s"], ["number", "word"])
        self.assertEqual(texts["Dig %d %s %d %s"], ["number", "word", "number", "word"])
        # a part this reader does not evaluate: nothing more
        toks = datfiles.lua_lex('local k = key("up"); des.engraving({ text = "Use " .. k })')
        at = next(i for i, t in enumerate(toks) if t.text == "engraving")
        expr = datfiles.engraving_texts(datfiles.lua_args(toks, at + 1))[0]
        self.assertEqual(datfiles.local_texts(toks, expr, at), [])


if __name__ == "__main__":
    unittest.main()
