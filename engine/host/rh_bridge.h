/* renethack engine host: libnh shim <-> protocol bridge */
#ifndef RH_BRIDGE_H
#define RH_BRIDGE_H

/* emit hello; call before nhmain() (the catalog follows at init_nhwindows) */
void rh_bridge_start(void);
/* the shim_graphics_set_callback() target */
void rh_bridge_callback(const char *name, void *ret_ptr, const char *fmt, ...);
/* atexit(): tell the client we are leaving on purpose */
void rh_bridge_atexit(void);
/* rh_catalog.c: malloc'd catalog message line, {"t":"catalog","a":{...}} */
char *rh_catalog_line(void);

#endif /* RH_BRIDGE_H */
