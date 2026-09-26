/* renethack engine host: process entry point.
 * The protocol owns the original stdin/stdout; the game itself sees
 * /dev/null and stderr there, so a stray printf() cannot corrupt a frame. */
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include "rh_proto.h"
#include "rh_bridge.h"

typedef void (*shim_callback_t)(const char *, void *, const char *, ...);
extern void shim_graphics_set_callback(shim_callback_t);
extern int nhmain(int, char **);

int main(int, char **);

int
main(int argc, char **argv)
{
    int out_fd = dup(STDOUT_FILENO), in_fd = dup(STDIN_FILENO), devnull;

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
