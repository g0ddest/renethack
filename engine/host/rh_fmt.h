/* renethack engine host: a message's printf format and its arguments, for
 * the client's translator (the nhevents pline_format hook).
 * Deliberately free of NetHack headers so it can be unit-tested alone. */
#ifndef RH_FMT_H
#define RH_FMT_H

#include <stdarg.h>
#include <stddef.h>
#include "cJSON.h"

/* What `ap` holds for `fmt`: a JSON array with one value per conversion,
   in order.  %s gives a string (null for a null pointer; cut to its
   precision, as printed), the integer conversions a number, %c a string
   of that character, the floating ones a number, %p a string; a '*' width
   or precision is read and left out; %% gives nothing.  NULL when the
   format has anything else (%n, wide characters, positional arguments):
   the caller then sends no arguments. */
cJSON *rh_fmt_args(const char *fmt, va_list ap);

/* The text vpline() shows for `fmt` and `ap`: printed, and when longer
   than `limit` characters (BUFSZ - 1) cut the way vpline() cuts it, to
   `limit` with "..." before the last three characters.  malloc'd; NULL
   when the format cannot be printed or memory runs out. */
char *rh_fmt_text(const char *fmt, va_list ap, size_t limit);

#endif /* RH_FMT_H */
