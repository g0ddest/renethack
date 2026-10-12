/* renethack engine host, Windows: mingw's time.h defines time() inline, so
 * rh_time.c's test clock cannot take its place at link time as on unix.
 * windows.mk includes this first in every file it compiles: their calls go
 * to rh_time(), the name rh_time.c's definition takes here. */
#ifndef RH_WINTIME_H
#define RH_WINTIME_H
#include <time.h>
time_t rh_time(time_t *);
#define time(t) rh_time(t)
#endif
