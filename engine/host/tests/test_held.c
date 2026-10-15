/* unit tests for rh_held.c (no NetHack involved) */
#include <stdio.h>
#include "rh_held.h"

static int failures;

#define CHECK(cond)                                                     \
    do {                                                                \
        if (!(cond)) {                                                  \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__,      \
                    __LINE__, #cond);                                   \
            failures++;                                                 \
        }                                                               \
    } while (0)

/* a monster told holding something, then nothing, is forgotten */
static void
test_told_and_forgotten(void)
{
    rh_held_clear();
    CHECK(rh_held_get(7) == RH_HELD_NONE);
    rh_held_set(7, 1234);
    CHECK(rh_held_get(7) == 1234);
    rh_held_set(7, 1240);
    CHECK(rh_held_get(7) == 1240);
    CHECK(rh_held_count() == 1);
    rh_held_set(7, RH_HELD_NONE);
    CHECK(rh_held_get(7) == RH_HELD_NONE);
    CHECK(rh_held_count() == 0);
}

/* 300 soldiers made, told armed, and killed one after another: the
   table keeps those still there, never grows past its size, and a
   wielder told after them all is remembered */
static void
test_the_dead_are_swept(void)
{
    unsigned id;

    rh_held_clear();
    for (id = 1; id <= 300; id++) {
        rh_held_set(id, 1000);
        /* the flush before the next request: only this one is there */
        rh_held_unmark();
        rh_held_mark(id);
        rh_held_sweep();
        CHECK(rh_held_count() <= 1);
    }
    CHECK(rh_held_get(300) == 1000);
    CHECK(rh_held_get(1) == RH_HELD_NONE);
    rh_held_set(301, 2000);
    CHECK(rh_held_get(301) == 2000);
}

/* told past its size without a sweep, it stops at its size; a sweep of
   the ones gone makes room again */
static void
test_full_then_swept(void)
{
    unsigned id;

    rh_held_clear();
    for (id = 1; id <= 300; id++)
        rh_held_set(id, 1000);
    CHECK(rh_held_count() == RH_HELD_MAX);
    CHECK(rh_held_get(300) == RH_HELD_NONE);
    rh_held_unmark();
    for (id = 1; id <= 10; id++)
        rh_held_mark(id);
    rh_held_sweep();
    CHECK(rh_held_count() == 10);
    CHECK(rh_held_get(5) == 1000);
    CHECK(rh_held_get(11) == RH_HELD_NONE);
    rh_held_set(400, 3000);
    CHECK(rh_held_get(400) == 3000);
    CHECK(rh_held_count() == 11);
}

int
main(void)
{
    test_told_and_forgotten();
    test_the_dead_are_swept();
    test_full_then_swept();
    if (failures) {
        fprintf(stderr, "test_held: %d failure(s)\n", failures);
        return 1;
    }
    printf("test_held: ok\n");
    return 0;
}
