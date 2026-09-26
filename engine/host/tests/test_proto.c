/* unit tests for rh_proto.c (no NetHack involved) */
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include "rh_proto.h"

static int failures;

#define CHECK(cond)                                                     \
    do {                                                                \
        if (!(cond)) {                                                  \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__,      \
                    __LINE__, #cond);                                   \
            failures++;                                                 \
        }                                                               \
    } while (0)

static void
test_fnv1a64(void)
{
    /* reference vectors; nh-protocol's Rust tests use the same ones */
    CHECK(rh_fnv1a64("", 0) == 0xcbf29ce484222325ULL);
    CHECK(rh_fnv1a64("a", 1) == 0xaf63dc4c8601ec8cULL);
    CHECK(rh_fnv1a64("foobar", 6) == 0x85944171f73967e8ULL);
}

static void
test_utf8_sanitize(void)
{
    char *s;

    s = rh_utf8_sanitize("plain ascii");
    CHECK(strcmp(s, "plain ascii") == 0);
    free(s);
    s = rh_utf8_sanitize("\xD0\xBF\xD1\x80\xD0\xB8"); /* "при" */
    CHECK(strcmp(s, "\xD0\xBF\xD1\x80\xD0\xB8") == 0);
    free(s);
    s = rh_utf8_sanitize("a\xFF" "b"); /* stray byte */
    CHECK(strcmp(s, "a\xEF\xBF\xBD" "b") == 0);
    free(s);
    s = rh_utf8_sanitize("\xD0"); /* truncated sequence */
    CHECK(strcmp(s, "\xEF\xBF\xBD") == 0);
    free(s);
    s = rh_utf8_sanitize("\xED\xA0\x80"); /* UTF-16 surrogate */
    CHECK(strcmp(s, "\xEF\xBF\xBD\xEF\xBF\xBD\xEF\xBF\xBD") == 0);
    free(s);
}

static void
test_parse_reply(void)
{
    const char *err = (const char *) 0;
    cJSON *r;

    r = rh_proto_parse_reply("{\"id\":7,\"r\":{\"key\":104}}", 7, &err);
    CHECK(r != (cJSON *) 0);
    CHECK(rh_reply_int(r, "key", -1) == 104);
    CHECK(rh_reply_int(r, "missing", -5) == -5);
    CHECK(rh_reply_has(r, "key"));
    CHECK(!rh_reply_has(r, "text"));
    cJSON_Delete(r);

    r = rh_proto_parse_reply("{\"id\":7,\"r\":{\"text\":\"Elbereth\"}}", 7,
                             &err);
    CHECK(r && rh_reply_str(r, "text")
          && strcmp(rh_reply_str(r, "text"), "Elbereth") == 0);
    CHECK(r && rh_reply_str(r, "key") == (const char *) 0);
    cJSON_Delete(r);

    CHECK(rh_proto_parse_reply("{\"id\":8,\"r\":{}}", 7, &err) == 0);
    CHECK(err && strstr(err, "id"));
    CHECK(rh_proto_parse_reply("[1,2]", 7, &err) == 0);
    CHECK(rh_proto_parse_reply("not json", 7, &err) == 0);
    CHECK(rh_proto_parse_reply("{\"id\":7}", 7, &err) == 0);
    CHECK(err && strstr(err, "\"r\""));
}

static int lost_calls;
static void
on_lost(void)
{
    lost_calls++;
}

/* full round trip through real pipes: request out, reply in */
static void
test_request_roundtrip(void)
{
    int to_client[2], to_engine[2];
    char buf[512];
    ssize_t n;
    cJSON *r;
    const char *reply = "{\"id\":1,\"r\":{\"ch\":121}}\n";

    CHECK(pipe(to_client) == 0 && pipe(to_engine) == 0);
    rh_proto_init(to_client[1], to_engine[0]);
    rh_proto_set_lost_handler(on_lost);
    CHECK(write(to_engine[1], reply, strlen(reply))
          == (ssize_t) strlen(reply));

    r = rh_proto_request("yn_function", cJSON_CreateObject());
    CHECK(r != (cJSON *) 0);
    CHECK(rh_reply_int(r, "ch", 0) == 121);
    cJSON_Delete(r);

    n = read(to_client[0], buf, sizeof buf - 1);
    CHECK(n > 0);
    buf[n > 0 ? n : 0] = '\0';
    CHECK(strcmp(buf, "{\"t\":\"req\",\"id\":1,\"fn\":\"yn_function\","
                      "\"a\":{}}\n") == 0);

    /* client goes away: the handler runs once, later requests short-cut */
    close(to_engine[1]);
    CHECK(rh_proto_request("nhgetch", (cJSON *) 0) == (cJSON *) 0);
    CHECK(lost_calls == 1);
    CHECK(rh_proto_is_lost());
    CHECK(rh_proto_request("nhgetch", (cJSON *) 0) == (cJSON *) 0);
    CHECK(lost_calls == 1);
}

int
main(void)
{
    test_fnv1a64();
    test_utf8_sanitize();
    test_parse_reply();
    test_request_roundtrip();
    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("test_proto: all checks passed\n");
    return 0;
}
