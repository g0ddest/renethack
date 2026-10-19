/* what each monster in view was last told: see rh_held.h */
#include "rh_held.h"

static struct {
    unsigned id;
    int glyph;
    int peaceful;
    int marked;
} held[RH_HELD_MAX];
static int n_held;

static int
find(unsigned id)
{
    int i;

    for (i = 0; i < n_held; i++)
        if (held[i].id == id)
            return i;
    return -1;
}

int
rh_held_get(unsigned id)
{
    int i = find(id);

    return i < 0 ? RH_HELD_NONE : held[i].glyph;
}

/* the entry of monster `id`, made if need be (-1: the table is full) */
static int
entry(unsigned id)
{
    int i = find(id);

    if (i < 0) {
        if (n_held >= RH_HELD_MAX)
            return -1;
        i = n_held++;
        held[i].id = id;
        held[i].glyph = RH_HELD_NONE;
        held[i].peaceful = 0;
    }
    held[i].marked = 1;
    return i;
}

/* an entry told nothing any more goes */
static void
forget_if_empty(int i)
{
    if (held[i].glyph == RH_HELD_NONE && !held[i].peaceful)
        held[i] = held[--n_held];
}

void
rh_held_set(unsigned id, int glyph)
{
    int i = glyph == RH_HELD_NONE ? find(id) : entry(id);

    if (i < 0)
        return;
    held[i].glyph = glyph;
    forget_if_empty(i);
}

int
rh_held_peaceful(unsigned id)
{
    int i = find(id);

    return i >= 0 && held[i].peaceful;
}

void
rh_held_set_peaceful(unsigned id, int peaceful)
{
    int i = peaceful ? entry(id) : find(id);

    if (i < 0)
        return;
    held[i].peaceful = peaceful != 0;
    forget_if_empty(i);
}

void
rh_held_unmark(void)
{
    int i;

    for (i = 0; i < n_held; i++)
        held[i].marked = 0;
}

void
rh_held_mark(unsigned id)
{
    int i = find(id);

    if (i >= 0)
        held[i].marked = 1;
}

void
rh_held_sweep(void)
{
    int i = 0;

    while (i < n_held) {
        if (held[i].marked)
            i++;
        else
            held[i] = held[--n_held];
    }
}

int
rh_held_count(void)
{
    return n_held;
}

void
rh_held_clear(void)
{
    n_held = 0;
}
