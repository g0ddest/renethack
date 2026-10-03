/* renethack engine host: what the hero's progress notice may tell (see
 * rh_progress.h).  No NetHack headers: unit-tested alone. */
#include <stddef.h>
#include <string.h>
#include "rh_progress.h"

int
rh_achievement_shown(int ach, int gameover)
{
    if (ach == RH_ACH_MINE_PRIZE || ach == RH_ACH_SOKO_PRIZE)
        return gameover != 0;
    return 1;
}

const char *
rh_end_how(int gameover, const char *killer_name, int ascended, int alive)
{
    static const struct {
        const char *killer, *how;
    } endings[] = {
        { "ascended", "ascended" }, { "escaped", "escaped" },
        { "quit", "quit" },         { "panic", "panicked" },
        { "trickery", "tricked" },
    };
    size_t i;

    if (!gameover)
        return NULL;
    if (ascended)
        return "ascended";
    for (i = 0; alive && killer_name && i < sizeof endings / sizeof endings[0];
         i++)
        if (!strcmp(killer_name, endings[i].killer))
            return endings[i].how;
    return "died";
}
