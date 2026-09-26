/* renethack engine host: test clock.
 * RENETHACK_FIXED_TIME=<unix seconds> pins time() for every caller in the
 * process (moon phase, Friday 13th, night, ubirthday, Lua's os.time), which
 * replays need.  Unset, this is an ordinary wall clock.  POSIX only: the
 * executable's definition takes precedence over the C library's. */
#define _POSIX_C_SOURCE 200809L
#include <stdlib.h>
#include <sys/time.h>
#include <time.h>

time_t
time(time_t *tloc)
{
    const char *fixed = getenv("RENETHACK_FIXED_TIME");
    time_t now;

    if (fixed && *fixed) {
        now = (time_t) strtoll(fixed, (char **) 0, 10);
    } else {
        struct timeval tv;

        (void) gettimeofday(&tv, (void *) 0);
        now = (time_t) tv.tv_sec;
    }
    if (tloc)
        *tloc = now;
    return now;
}
