/* renethack engine for Windows: NetHack's dlb.h, with the data file named
 * "nhdat" as on every other platform.  windconf.h defines
 * VERSION_IN_DLB_FILENAME, and dlb_init() then opens nhdat<version>
 * ("nhdat500"), which the client's playground does not have: the engine
 * found no Lua and stopped at nhl_init.  windows.mk puts this directory
 * before NetHack's include/, so every file that includes dlb.h comes here
 * first, after config.h has read windconf.h. */
#undef VERSION_IN_DLB_FILENAME
#include_next "dlb.h"
