/* renethack engine host: JSON Lines protocol plumbing */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include "rh_proto.h"

static FILE *proto_out, *proto_in;
static unsigned long next_id = 1;
static void (*lost_handler)(void);
static int lost;

void
rh_proto_init(int out_fd, int in_fd)
{
    proto_out = fdopen(out_fd, "w");
    proto_in = fdopen(in_fd, "r");
    if (!proto_out || !proto_in) {
        perror("renethack: fdopen");
        exit(2);
    }
    (void) setvbuf(proto_out, (char *) 0, _IOFBF, 1 << 16);
}

void
rh_proto_set_lost_handler(void (*handler)(void))
{
    lost_handler = handler;
}

int
rh_proto_is_lost(void)
{
    return lost;
}

static void
emit(cJSON *msg)
{
    char *text = cJSON_PrintUnformatted(msg);

    if (text) {
        fputs(text, proto_out);
        fputc('\n', proto_out);
        cJSON_free(text);
    }
    cJSON_Delete(msg);
}

void
rh_proto_send(const char *type, const char *fn, cJSON *args)
{
    cJSON *msg = cJSON_CreateObject();

    cJSON_AddStringToObject(msg, "t", type);
    if (fn)
        cJSON_AddStringToObject(msg, "fn", fn);
    cJSON_AddItemToObject(msg, "a", args ? args : cJSON_CreateObject());
    emit(msg);
}

void
rh_proto_send_raw(const char *line)
{
    fputs(line, proto_out);
    fputc('\n', proto_out);
}

void
rh_proto_flush(void)
{
    if (proto_out)
        (void) fflush(proto_out);
}

static void
become_lost(const char *why)
{
    cJSON *args = cJSON_CreateObject();

    lost = 1;
    cJSON_AddStringToObject(args, "msg", why);
    rh_proto_send("error", (const char *) 0, args);
    rh_proto_flush();
    fprintf(stderr, "renethack: client lost: %s\n", why);
    if (lost_handler)
        lost_handler();
}

void
rh_proto_violation(const char *why)
{
    if (!lost)
        become_lost(why);
}

cJSON *
rh_proto_parse_reply(const char *line, unsigned long expect_id,
                     const char **err)
{
    cJSON *root = cJSON_Parse(line), *id, *r;

    if (!root || !cJSON_IsObject(root)) {
        *err = "reply is not a JSON object";
        cJSON_Delete(root);
        return (cJSON *) 0;
    }
    id = cJSON_GetObjectItemCaseSensitive(root, "id");
    if (!cJSON_IsNumber(id) || id->valuedouble != (double) expect_id) {
        *err = "reply id does not match the pending request";
        cJSON_Delete(root);
        return (cJSON *) 0;
    }
    r = cJSON_DetachItemFromObjectCaseSensitive(root, "r");
    cJSON_Delete(root);
    if (!r || !cJSON_IsObject(r)) {
        *err = "reply has no \"r\" object";
        cJSON_Delete(r);
        return (cJSON *) 0;
    }
    return r;
}

cJSON *
rh_proto_request(const char *fn, cJSON *args)
{
    unsigned long id;
    cJSON *msg, *r;
    char *line = (char *) 0;
    size_t cap = 0;
    const char *err = (const char *) 0;

    if (lost) {
        cJSON_Delete(args);
        return (cJSON *) 0;
    }
    id = next_id++;
    msg = cJSON_CreateObject();
    cJSON_AddStringToObject(msg, "t", "req");
    cJSON_AddNumberToObject(msg, "id", (double) id);
    cJSON_AddStringToObject(msg, "fn", fn);
    cJSON_AddItemToObject(msg, "a", args ? args : cJSON_CreateObject());
    emit(msg);
    rh_proto_flush();

    if (getline(&line, &cap, proto_in) < 0) {
        free(line);
        become_lost("client closed the connection");
        return (cJSON *) 0;
    }
    r = rh_proto_parse_reply(line, id, &err);
    free(line);
    if (!r)
        become_lost(err);
    return r;
}

long
rh_reply_int(const cJSON *r, const char *key, long fallback)
{
    const cJSON *v = cJSON_GetObjectItemCaseSensitive(r, key);

    return cJSON_IsNumber(v) ? (long) v->valuedouble : fallback;
}

const char *
rh_reply_str(const cJSON *r, const char *key)
{
    const cJSON *v = cJSON_GetObjectItemCaseSensitive(r, key);

    return cJSON_IsString(v) ? v->valuestring : (const char *) 0;
}

int
rh_reply_has(const cJSON *r, const char *key)
{
    return cJSON_GetObjectItemCaseSensitive(r, key) != (cJSON *) 0;
}

cJSON *
rh_json_string(const char *s)
{
    cJSON *item;
    char *clean;

    if (!s)
        return cJSON_CreateNull();
    clean = rh_utf8_sanitize(s);
    item = cJSON_CreateString(clean);
    free(clean);
    return item;
}

/* length of the valid UTF-8 sequence starting at p, or 0 if invalid */
static size_t
utf8_seq_len(const unsigned char *p)
{
    size_t n, i;
    unsigned long cp;

    if (p[0] < 0x80)
        return 1;
    if (p[0] >= 0xC2 && p[0] <= 0xDF)
        n = 2, cp = p[0] & 0x1F;
    else if (p[0] >= 0xE0 && p[0] <= 0xEF)
        n = 3, cp = p[0] & 0x0F;
    else if (p[0] >= 0xF0 && p[0] <= 0xF4)
        n = 4, cp = p[0] & 0x07;
    else
        return 0;
    for (i = 1; i < n; i++) {
        if ((p[i] & 0xC0) != 0x80)
            return 0;
        cp = (cp << 6) | (p[i] & 0x3F);
    }
    if ((n == 3 && cp < 0x800) || (n == 4 && (cp < 0x10000 || cp > 0x10FFFF))
        || (cp >= 0xD800 && cp <= 0xDFFF))
        return 0;
    return n;
}

char *
rh_utf8_sanitize(const char *s)
{
    const unsigned char *p = (const unsigned char *) s;
    char *out = malloc(strlen(s) * 3 + 1), *o = out;
    size_t n;

    if (!out) {
        perror("renethack: malloc");
        exit(2);
    }
    while (*p) {
        n = utf8_seq_len(p);
        if (n) {
            memcpy(o, p, n);
            o += n, p += n;
        } else {
            memcpy(o, "\xEF\xBF\xBD", 3); /* U+FFFD */
            o += 3, p += 1;
        }
    }
    *o = '\0';
    return out;
}

unsigned long long
rh_fnv1a64(const char *data, size_t len)
{
    unsigned long long h = 0xcbf29ce484222325ULL;
    size_t i;

    for (i = 0; i < len; i++) {
        h ^= (unsigned char) data[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}
