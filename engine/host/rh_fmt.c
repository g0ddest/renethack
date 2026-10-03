/* renethack engine host: a message's printf format and its arguments (see
 * rh_fmt.h).  No NetHack headers: unit-tested alone. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rh_proto.h"
#include "rh_fmt.h"

/* the length modifiers of a conversion */
enum rh_len { LEN_NONE, LEN_HH, LEN_H, LEN_L, LEN_LL, LEN_J, LEN_Z, LEN_T,
              LEN_BIG_L };

/* the argument of one integer conversion */
static double
int_arg(va_list *ap, enum rh_len len, int is_signed)
{
    switch (len) {
    case LEN_HH:
        return is_signed ? (double) (signed char) va_arg(*ap, int)
                         : (double) (unsigned char) va_arg(*ap, int);
    case LEN_H:
        return is_signed ? (double) (short) va_arg(*ap, int)
                         : (double) (unsigned short) va_arg(*ap, int);
    case LEN_L:
        return is_signed ? (double) va_arg(*ap, long)
                         : (double) va_arg(*ap, unsigned long);
    case LEN_LL:
        return is_signed ? (double) va_arg(*ap, long long)
                         : (double) va_arg(*ap, unsigned long long);
    case LEN_J:
        return is_signed ? (double) va_arg(*ap, intmax_t)
                         : (double) va_arg(*ap, uintmax_t);
    case LEN_Z: {
        size_t z = va_arg(*ap, size_t);

        return is_signed ? (double) (ptrdiff_t) z : (double) z;
    }
    case LEN_T: {
        ptrdiff_t t = va_arg(*ap, ptrdiff_t);

        return is_signed ? (double) t : (double) (size_t) t;
    }
    default:
        return is_signed ? (double) va_arg(*ap, int)
                         : (double) va_arg(*ap, unsigned int);
    }
}

/* `s` up to `prec` bytes (all of it when prec < 0) */
static cJSON *
string_arg(const char *s, int prec)
{
    char *cut;
    size_t n = 0;
    cJSON *v;

    if (!s || prec < 0)
        return rh_json_string(s);
    while (n < (size_t) prec && s[n])
        n++;
    cut = malloc(n + 1);
    if (!cut)
        return NULL;
    memcpy(cut, s, n);
    cut[n] = '\0';
    v = rh_json_string(cut);
    free(cut);
    return v;
}

/* one conversion after its '%': its value, read from `ap`, and the format
   moved past it; NULL when the conversion is not one we know */
static cJSON *
conversion(const char **fp, va_list *ap)
{
    const char *f = *fp;
    int prec = -1;
    enum rh_len len = LEN_NONE;
    cJSON *v = NULL;
    char c[2];

    while (*f && strchr("-+ #0'", *f))
        f++;
    if (*f == '*') {
        (void) va_arg(*ap, int);
        f++;
    } else {
        while (*f >= '0' && *f <= '9')
            f++;
    }
    if (*f == '.') {
        f++;
        if (*f == '*') {
            prec = va_arg(*ap, int); /* negative: as if none */
            f++;
        } else {
            for (prec = 0; *f >= '0' && *f <= '9'; f++)
                prec = prec * 10 + (*f - '0');
        }
    }
    if (*f == 'h' || *f == 'l') {
        int twice = f[1] == *f;

        if (*f == 'h')
            len = twice ? LEN_HH : LEN_H;
        else
            len = twice ? LEN_LL : LEN_L;
        f += twice ? 2 : 1;
    } else if (*f && strchr("jztL", *f)) {
        len = *f == 'j' ? LEN_J
              : *f == 'z' ? LEN_Z
              : *f == 't' ? LEN_T
              : LEN_BIG_L;
        f++;
    }
    switch (*f) {
    case 'd': case 'i':
        if (len != LEN_BIG_L)
            v = cJSON_CreateNumber(int_arg(ap, len, 1));
        break;
    case 'u': case 'o': case 'x': case 'X':
        if (len != LEN_BIG_L)
            v = cJSON_CreateNumber(int_arg(ap, len, 0));
        break;
    case 'c':
        if (len == LEN_NONE) {
            c[0] = (char) va_arg(*ap, int);
            c[1] = '\0';
            v = rh_json_string(c);
        }
        break;
    case 's':
        if (len == LEN_NONE)
            v = string_arg(va_arg(*ap, const char *), prec);
        break;
    case 'f': case 'F': case 'e': case 'E':
    case 'g': case 'G': case 'a': case 'A':
        if (len == LEN_BIG_L)
            v = cJSON_CreateNumber((double) va_arg(*ap, long double));
        else if (len == LEN_NONE || len == LEN_L)
            v = cJSON_CreateNumber(va_arg(*ap, double));
        break;
    case 'p':
        if (len == LEN_NONE) {
            char buf[64];

            (void) snprintf(buf, sizeof buf, "%p", va_arg(*ap, void *));
            v = rh_json_string(buf);
        }
        break;
    }
    if (v)
        *fp = f + 1;
    return v;
}

cJSON *
rh_fmt_args(const char *fmt, va_list ap)
{
    cJSON *args = cJSON_CreateArray(), *v;
    va_list walk;
    const char *f = fmt;

    if (!args || !fmt) {
        cJSON_Delete(args);
        return NULL;
    }
    va_copy(walk, ap);
    while ((f = strchr(f, '%')) != NULL) {
        f++;
        if (*f == '%') {
            f++;
            continue;
        }
        if (!(v = conversion(&f, &walk))) {
            cJSON_Delete(args);
            args = NULL;
            break;
        }
        cJSON_AddItemToArray(args, v);
    }
    va_end(walk);
    return args;
}

/* printing a format that came from the game is the point here (as in
   vpline(), which also turns this warning off) */
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wformat-nonliteral"
#endif

char *
rh_fmt_text(const char *fmt, va_list ap, size_t limit)
{
    va_list ap2;
    char *text;
    int n;

    va_copy(ap2, ap);
    n = vsnprintf(NULL, 0, fmt, ap2);
    va_end(ap2);
    if (n < 0 || !(text = malloc((size_t) n + 1)))
        return NULL;
    va_copy(ap2, ap);
    (void) vsnprintf(text, (size_t) n + 1, fmt, ap2);
    va_end(ap2);
    if ((size_t) n > limit && limit >= 6) {
        /* "___ extremely long text" -> "___ extremely l...ext" */
        memcpy(text + limit - 6, "...", 3);
        memmove(text + limit - 3, text + n - 3, 3);
        text[limit] = '\0';
    }
    return text;
}

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#endif
