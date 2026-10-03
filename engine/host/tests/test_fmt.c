/* unit tests for rh_fmt.c (no NetHack involved) */
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rh_fmt.h"

static int failures;

#define CHECK(cond)                                                     \
    do {                                                                \
        if (!(cond)) {                                                  \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__,      \
                    __LINE__, #cond);                                   \
            failures++;                                                 \
        }                                                               \
    } while (0)

/* rh_fmt_args() of the format and these arguments, as a JSON line
   ("none" when it gives up); the caller frees it */
static char *
args_of(const char *fmt, ...)
{
    va_list ap;
    cJSON *args;
    char *line;

    va_start(ap, fmt);
    args = rh_fmt_args(fmt, ap);
    va_end(ap);
    if (!args) {
        line = malloc(sizeof "none");
        strcpy(line, "none");
        return line;
    }
    line = cJSON_PrintUnformatted(args);
    cJSON_Delete(args);
    return line;
}

/* rh_fmt_text() of the format and these arguments, cut at 255 */
static char *
text_of(const char *fmt, ...)
{
    va_list ap;
    char *text;

    va_start(ap, fmt);
    text = rh_fmt_text(fmt, ap, 255);
    va_end(ap);
    return text;
}

static int
gives(char *got, const char *want)
{
    int same = got && !strcmp(got, want);

    if (!same)
        fprintf(stderr, "  got %s, want %s\n", got ? got : "(null)", want);
    free(got);
    return same;
}

static void
test_each_conversion_gives_one_value(void)
{
    CHECK(gives(args_of("You hit %s.", "the newt"), "[\"the newt\"]"));
    CHECK(gives(args_of("You see here %s (%d zorkmids).", "a lamp", 10),
                "[\"a lamp\",10]"));
    CHECK(gives(args_of("%c - %s.", 'a', "a +1 long sword"),
                "[\"a\",\"a +1 long sword\"]"));
    CHECK(gives(args_of("%ld %lu %lld %llu", -7L, 8UL, -9LL, 10ULL),
                "[-7,8,-9,10]"));
    CHECK(gives(args_of("%hhd %hd %hu %zu %x %X %o", 300, 70000, 70000,
                        (size_t) 5, 255, 255U, 8U),
                "[44,4464,4464,5,255,255,8]"));
    CHECK(gives(args_of("%-10s|%5d|%05.1f|%+i|% d|%#x", "x", 3, 2.5, 4, 5,
                        16),
                "[\"x\",3,2.5,4,5,16]"));
    CHECK(gives(args_of("%g %e %Lf", 0.25, 1e3, (long double) 2.0),
                "[0.25,1000,2]"));
    /* a null string is null, not "(null)" */
    CHECK(gives(args_of("%s", (char *) 0), "[null]"));
    /* the format "%s" is a format like any other */
    CHECK(gives(args_of("%s", "It's a wall."), "[\"It's a wall.\"]"));
}

static void
test_stars_precision_and_percent_signs(void)
{
    /* '*' is read and left out: args[i] is the i-th conversion */
    CHECK(gives(args_of("%*d|%-*s", 5, 42, 3, "ab"), "[42,\"ab\"]"));
    /* a precision cuts a string, as printed */
    CHECK(gives(args_of("%.3s|%.*s", "abcdef", 2, "xyz"), "[\"abc\",\"xy\"]"));
    CHECK(gives(args_of("%.*s", -1, "whole"), "[\"whole\"]"));
    CHECK(gives(args_of("100%% sure, %d%%", 7), "[7]"));
    CHECK(gives(args_of("no conversions"), "[]"));
}

static void
test_unknown_conversions_give_nothing(void)
{
    int n;

    CHECK(gives(args_of("%n", &n), "none"));
    CHECK(gives(args_of("%ls", L"wide"), "none"));
    CHECK(gives(args_of("%1$s", "positional"), "none"));
    CHECK(gives(args_of("%Ld", 1), "none"));
    CHECK(gives(args_of("%y", 1), "none"));
    CHECK(gives(args_of("trailing %"), "none"));
}

static void
test_the_text_is_vpline_s(void)
{
    char longer[400], want[256];
    char *got;
    int i;

    CHECK(gives(text_of("You hit %s.", "the newt"), "You hit the newt."));
    CHECK(gives(text_of("plain"), "plain"));
    /* longer than 255: the first 249, "...", the last three */
    for (i = 0; i < 300; i++)
        longer[i] = (char) ('a' + i % 26);
    longer[300] = '\0';
    memcpy(want, longer, 249);
    memcpy(want + 249, "...", 3);
    memcpy(want + 252, longer + 297, 3);
    want[255] = '\0';
    CHECK(gives(text_of("%s", longer), want));
    /* 255 exactly is left alone */
    longer[255] = '\0';
    got = text_of("%s", longer);
    CHECK(got && strlen(got) == 255 && !strcmp(got, longer));
    free(got);
}

int
main(void)
{
    test_each_conversion_gives_one_value();
    test_stars_precision_and_percent_signs();
    test_unknown_conversions_give_nothing();
    test_the_text_is_vpline_s();
    if (failures) {
        fprintf(stderr, "test_fmt: %d failure(s)\n", failures);
        return 1;
    }
    printf("test_fmt: ok\n");
    return 0;
}
