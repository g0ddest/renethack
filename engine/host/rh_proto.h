/* renethack engine host: JSON Lines protocol plumbing.
 * Deliberately free of NetHack headers so it can be unit-tested alone. */
#ifndef RH_PROTO_H
#define RH_PROTO_H

#include <stddef.h>
#include "cJSON.h"

#define RH_PROTOCOL_VERSION 1

/* take over the given descriptors for protocol traffic */
void rh_proto_init(int out_fd, int in_fd);
/* runs once when the client disappears (EOF) or sends a broken reply */
void rh_proto_set_lost_handler(void (*handler)(void));
int rh_proto_is_lost(void);
/* report a reply that parsed but makes no sense; same outcome as EOF */
void rh_proto_violation(const char *why);

/* write {"t":type,"fn":fn,"a":args}; fn may be NULL; takes ownership of args */
void rh_proto_send(const char *type, const char *fn, cJSON *args);
/* write an already serialized message line (the catalog, so its hash is exact) */
void rh_proto_send_raw(const char *line);
void rh_proto_flush(void);
/* send {"t":"req","id":N,"fn":fn,"a":args}, wait for {"id":N,"r":{...}};
   returns the detached "r" object (caller deletes it), or NULL once the
   client is lost -- callers then answer the game with a cancel value */
cJSON *rh_proto_request(const char *fn, cJSON *args);

/* parse one reply line; returns the detached "r" object or NULL with *err set */
cJSON *rh_proto_parse_reply(const char *line, unsigned long expect_id,
                            const char **err);
long rh_reply_int(const cJSON *r, const char *key, long fallback);
const char *rh_reply_str(const cJSON *r, const char *key);
int rh_reply_has(const cJSON *r, const char *key);

cJSON *rh_json_string(const char *s); /* NULL -> JSON null; sanitizes UTF-8 */
char *rh_utf8_sanitize(const char *s); /* malloc'd; bad bytes -> U+FFFD */
unsigned long long rh_fnv1a64(const char *data, size_t len);

#endif /* RH_PROTO_H */
