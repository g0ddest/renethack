/* renethack engine host: the POSIX calls the host makes that Windows' C
 * library lacks (mingw-w64 builds only). */
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>

int setenv(const char *, const char *, int);
int unsetenv(const char *);
ssize_t getline(char **, size_t *, FILE *);

int
setenv(const char *name, const char *value, int overwrite)
{
    if (!overwrite && getenv(name))
        return 0;
    return _putenv_s(name, value) ? -1 : 0;
}

int
unsetenv(const char *name)
{
    return _putenv_s(name, "") ? -1 : 0;
}

/* the line with its '\n', in a buffer grown as needed; -1 at the end */
ssize_t
getline(char **line, size_t *cap, FILE *f)
{
    size_t len = 0;
    int c;

    if (!*line || !*cap) {
        *cap = 128;
        if (!(*line = realloc(*line, *cap)))
            return -1;
    }
    while ((c = getc(f)) != EOF) {
        if (len + 2 > *cap) {
            char *more = realloc(*line, *cap * 2);

            if (!more) {
                errno = ENOMEM;
                return -1;
            }
            *line = more;
            *cap *= 2;
        }
        (*line)[len++] = (char) c;
        if (c == '\n')
            break;
    }
    (*line)[len] = '\0';
    return len ? (ssize_t) len : -1;
}
