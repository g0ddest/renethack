/* unit tests for rh_progress.c (no NetHack involved) */
#include <stdio.h>
#include <string.h>
#include "rh_progress.h"

static int failures;

#define CHECK(cond)                                                     \
    do {                                                                \
        if (!(cond)) {                                                  \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__,      \
                    __LINE__, #cond);                                   \
            failures++;                                                 \
        }                                                               \
    } while (0)

static int
same(const char *a, const char *b)
{
    return a && b && !strcmp(a, b);
}

static void
test_spoilers_wait_for_the_end(void)
{
    int ach;

    /* the Mines' End luckstone and the Sokoban prize: only at the end */
    CHECK(!rh_achievement_shown(RH_ACH_MINE_PRIZE, 0));
    CHECK(!rh_achievement_shown(RH_ACH_SOKO_PRIZE, 0));
    CHECK(rh_achievement_shown(RH_ACH_MINE_PRIZE, 1));
    CHECK(rh_achievement_shown(RH_ACH_SOKO_PRIZE, 1));
    /* every other one at once */
    for (ach = 1; ach <= 31; ach++)
        if (ach != RH_ACH_MINE_PRIZE && ach != RH_ACH_SOKO_PRIZE)
            CHECK(rh_achievement_shown(ach, 0));
}

static void
test_how_a_game_ended(void)
{
    CHECK(rh_end_how(0, "quit", 0, 1) == NULL);
    CHECK(same(rh_end_how(1, "quit", 0, 1), "quit"));
    CHECK(same(rh_end_how(1, "escaped", 0, 1), "escaped"));
    CHECK(same(rh_end_how(1, "ascended", 1, 1), "ascended"));
    CHECK(same(rh_end_how(1, "panic", 0, 1), "panicked"));
    CHECK(same(rh_end_how(1, "trickery", 0, 1), "tricked"));
    CHECK(same(rh_end_how(1, "jackal", 0, 0), "died"));
    CHECK(same(rh_end_how(1, "drowning", 0, 0), "died"));
    /* a pet named "quit" that kills the hero: a death */
    CHECK(same(rh_end_how(1, "quit", 0, 0), "died"));
    CHECK(same(rh_end_how(1, NULL, 0, 0), "died"));
}

int
main(void)
{
    test_spoilers_wait_for_the_end();
    test_how_a_game_ended();
    if (failures) {
        fprintf(stderr, "test_progress: %d failure(s)\n", failures);
        return 1;
    }
    printf("test_progress: ok\n");
    return 0;
}
