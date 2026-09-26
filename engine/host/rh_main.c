/* renethack engine host: process entry point.
 * The protocol owns the original stdin/stdout; the game itself sees
 * /dev/null and stderr there, so a stray printf() cannot corrupt a frame. */
#define _POSIX_C_SOURCE 200809L /* setenv, unsetenv, tzset, getcwd */
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include "rh_proto.h"
#include "rh_bridge.h"

typedef void (*shim_callback_t)(const char *, void *, const char *, ...);
extern void shim_graphics_set_callback(shim_callback_t);
extern int nhmain(int, char **);

int main(int, char **);

/* The same options, seed and clock must give the same game for every player
 * on every machine, and replays depend on it:
 * - HOME is the playground, so a personal ~/.nethackrc cannot change
 *   options, key bindings or symbols;
 * - no mail: a Unix mailbox must never summon NetHack's mail daemon;
 * - with a fixed clock the local time zone is UTC, because the moon phase,
 *   Friday 13th and night come from localtime(). */
static void
pin_environment(void)
{
    char cwd[4096], *opts;
    const char *given = getenv("NETHACKOPTIONS"),
               *fixed = getenv("RENETHACK_FIXED_TIME");

    if (getcwd(cwd, sizeof cwd))
        (void) setenv("HOME", cwd, 1);
    (void) unsetenv("MAIL");
    if (!given || *given != '@') { /* '@file' names an options file */
        opts = malloc(sizeof "!mail," + (given ? strlen(given) : 0));
        if (opts) {
            strcpy(opts, "!mail");
            if (given && *given) {
                strcat(opts, ",");
                strcat(opts, given);
            }
            (void) setenv("NETHACKOPTIONS", opts, 1);
            free(opts);
        }
    }
    if (fixed && *fixed) {
        (void) setenv("TZ", "UTC0", 1);
        tzset();
    }
}

int
main(int argc, char **argv)
{
    int out_fd, in_fd, devnull;

    pin_environment();
    out_fd = dup(STDOUT_FILENO);
    in_fd = dup(STDIN_FILENO);

    if (out_fd < 0 || in_fd < 0) {
        perror("renethack: dup");
        return 2;
    }
    (void) dup2(STDERR_FILENO, STDOUT_FILENO);
    devnull = open("/dev/null", O_RDONLY);
    if (devnull >= 0) {
        (void) dup2(devnull, STDIN_FILENO);
        (void) close(devnull);
    }
    /* a vanished client must surface as EOF/EPIPE, not kill us mid-save */
    (void) signal(SIGPIPE, SIG_IGN);

    rh_proto_init(out_fd, in_fd);
    rh_bridge_start();
    (void) atexit(rh_bridge_atexit);
    shim_graphics_set_callback(rh_bridge_callback);
    return nhmain(argc, argv);
}
