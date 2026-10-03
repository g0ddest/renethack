/* renethack engine host: forwards libnh shim window calls over the protocol.
 * Every shim_* call arrives here as (name, return slot, format, varargs);
 * the argument types follow the prototypes in win/shim/winshim.c. */
#include "hack.h"
#include "func_tab.h"
#include "dlb.h"
#include <stdarg.h>
#include "rh_proto.h"
#include "rh_bridge.h"
#include "rh_progress.h"

/* rh_progress.h names NetHack's two spoiler achievements by number */
typedef char rh_spoiler_ids_match[(ACH_MINE_PRIZE == RH_ACH_MINE_PRIZE
                                   && ACH_SOKO_PRIZE == RH_ACH_SOKO_PRIZE)
                                      ? 1 : -1];

#define RH_MAX_WINDOWS 64

struct rh_window {
    int type;          /* NHW_*; 0 marks a free slot */
    anything *ids;     /* menu item identifiers, by item index */
    int nitems, maxitems;
};

static struct rh_window windows[RH_MAX_WINDOWS];

extern glyph_map glyphmap[MAX_GLYPH];

/* the inventory changed since the last snapshot the client got */
static boolean inventory_dirty;
/* program_state.in_moveloop as of the last request: a rise means a new or
   restored game, whose inventory the client has not seen */
static boolean seen_moveloop;
/* the level the client was last told the hero is on (dnum < 0: none) */
static d_level told_level = { -1, -1 };
/* the hash of the progress notice the client last got (0: none) */
static unsigned long long told_progress;

static const char *const status_names[MAXBLSTATS] = {
    "title", "str", "dex", "con", "int", "wis", "cha",
    "align", "score", "cap", "gold",
    "energy", "energymax", "xlevel", "ac", "hitdice",
    "time", "hunger", "hp", "hpmax",
    "leveldesc", "exp", "condition",
    "weapon", "armor", "terrain",
    "version",
};

static const char *
wintype_name(int type)
{
    switch (type) {
    case NHW_MESSAGE: return "message";
    case NHW_STATUS: return "status";
    case NHW_MAP: return "map";
    case NHW_MENU: return "menu";
    case NHW_TEXT: return "text";
    case NHW_PERMINVENT: return "perminvent";
    default: return "unknown";
    }
}

static cJSON *
args_new(void)
{
    return cJSON_CreateObject();
}

static void
add_int(cJSON *o, const char *key, long v)
{
    cJSON_AddNumberToObject(o, key, (double) v);
}

static void
add_str(cJSON *o, const char *key, const char *s)
{
    cJSON_AddItemToObject(o, key, rh_json_string(s));
}

static void
add_bool(cJSON *o, const char *key, int v)
{
    cJSON_AddBoolToObject(o, key, v ? 1 : 0);
}

/* What the glyph depicts.  Objects deliberately carry no glyph number: it
   encodes the true object type.  Their tile is the appearance tile, shared
   by every object that looks the same (rh_object_appearance_tile). */
static cJSON *
glyph_json(const glyph_info *ginfo)
{
    cJSON *o;
    int g, tile;
    const char *kind = "other";

    if (!ginfo)
        return cJSON_CreateNull();
    g = ginfo->glyph;
    tile = ginfo->gm.tileidx;
    o = args_new();
    add_int(o, "ch", ginfo->ttychar);
    add_int(o, "color", ginfo->gm.sym.color);
    add_int(o, "flags", (long) ginfo->gm.glyphflags);
    if (glyph_is_unexplored(g)) {
        kind = "unexplored";
    } else if (glyph_is_nothing(g)) {
        kind = "nothing";
    } else if (glyph_is_invisible(g)) {
        kind = "invisible";
    } else if (glyph_is_warning(g)) {
        kind = "warning";
        add_int(o, "level", glyph_to_warning(g));
    } else if (glyph_is_swallow(g)) {
        kind = "swallow";
    } else if (glyph_is_explosion(g)) {
        kind = "explosion";
    } else if (glyph_is_cmap_zap(g)) {
        kind = "zap";
    } else if (glyph_is_cmap(g)) {
        kind = "cmap";
        add_int(o, "cmap", glyph_to_cmap(g));
    } else if (glyph_is_statue(g)) {
        kind = "statue";
        add_int(o, "mon", glyph_to_statue_corpsenm(g));
    } else if (glyph_is_body(g)) {
        kind = "body";
        add_int(o, "mon", glyph_to_body_corpsenm(g));
    } else if (glyph_is_object(g)) {
        kind = "obj";
        tile = rh_object_appearance_tile(tile);
    } else if (glyph_is_monster(g)) {
        kind = "mon";
        add_int(o, "mon", glyph_to_mon(g));
    }
    add_int(o, "tile", tile);
    if (strcmp(kind, "obj") != 0)
        add_int(o, "glyph", g);
    cJSON_AddStringToObject(o, "kind", kind);
    return o;
}

static int
valid_win(int w)
{
    return w > 0 && w < RH_MAX_WINDOWS && windows[w].type != 0;
}

/* The inventory snapshot (P6 of the spec), done here instead of in an
   engine patch.  It is what the perm_invent option makes every windowport
   do: doname() on the whole inventory at each update, which may set
   dknown exactly as displaying the inventory does.  Nothing identifying is
   sent: the tile is the appearance tile (as glyph_json() does for objects),
   the class is apparent from the appearance, the text is what the
   character sees; no glyph number, object type or weight. */
static const struct {
    long mask;
    const char *name;
} worn_slots[] = {
    { W_WEP, "weapon" },      { W_SWAPWEP, "alternate" },
    { W_QUIVER, "quiver" },   { W_ARM, "body" },
    { W_ARMC, "cloak" },      { W_ARMH, "helmet" },
    { W_ARMS, "shield" },     { W_ARMG, "gloves" },
    { W_ARMF, "boots" },      { W_ARMU, "shirt" },
    { W_AMUL, "amulet" },     { W_RINGL, "left_ring" },
    { W_RINGR, "right_ring" }, { W_TOOL, "eyes" },
    { W_BALL, "ball" },       { W_CHAIN, "chain" },
};

static cJSON *
inventory_json(void)
{
    cJSON *a = args_new(), *items = cJSON_AddArrayToObject(a, "items");
    cJSON *o, *slots;
    struct obj *otmp;
    char letter[2], cls[2], *name;
    int glyph, tile, save_suppress_price = iflags.suppress_price;
    size_t i;

    /* as update_inventory() does: prices in full whatever the caller was
       formatting */
    iflags.suppress_price = 0;
    for (otmp = gi.invent; otmp; otmp = otmp->nobj) {
        o = args_new();
        letter[0] = otmp->invlet;
        letter[1] = '\0';
        cJSON_AddStringToObject(o, "letter", letter);
        cls[0] = def_oc_syms[(int) otmp->oclass].sym;
        cls[1] = '\0';
        cJSON_AddStringToObject(o, "class", cls);
        glyph = obj_to_glyph(otmp, rn2_on_display_rng);
        tile = glyphmap[glyph].tileidx;
        if (glyph_is_object(glyph))
            tile = rh_object_appearance_tile(tile);
        add_int(o, "tile", tile);
        add_int(o, "quan", otmp->quan);
        slots = cJSON_AddArrayToObject(o, "slots");
        for (i = 0; i < SIZE(worn_slots); i++)
            if (otmp->owornmask & worn_slots[i].mask)
                cJSON_AddItemToArray(slots,
                                     cJSON_CreateString(worn_slots[i].name));
        add_bool(o, "lit", otmp->lamplit);
        name = doname(otmp);
        add_str(o, "text", name);
        /* give the obuf back, as display_pickinv() does: a snapshot must
           not clobber names the core is still holding */
        maybereleaseobuf(name);
        cJSON_AddItemToArray(items, o);
    }
    iflags.suppress_price = save_suppress_price;
    /* the alternate weapon is then wielded in the off hand */
    add_bool(a, "twoweap", u.twoweap);
    return a;
}

/* Before the engine waits for input.  Callers build their own arguments
   first: strings they were given may live in objnam.c's obufs, which the
   snapshot's doname() calls reuse.  Never from update_inventory itself,
   which the core calls in the middle of changing things. */
static void
inventory_flush(void)
{
    if (!program_state.in_moveloop) {
        seen_moveloop = FALSE;
        return;
    }
    if (!seen_moveloop) {
        seen_moveloop = TRUE;
        inventory_dirty = TRUE;
    }
    /* saving or restoring: update_inventory() is off, and the chain may
       be half freed */
    if (!inventory_dirty || rh_proto_is_lost() || suppress_map_output())
        return;
    inventory_dirty = FALSE;
    rh_proto_send("win", "inventory", inventory_json());
}

/* Before the engine waits for input: the hero arrived on another level
   (or a game began or was restored).  Only the branch and the depth, which
   the hero always knows (the overview names the branch on arrival); never
   a special level's name, which would tell more than the map shows. */
static void
level_flush(void)
{
    cJSON *a;

    if (!program_state.in_moveloop) {
        told_level.dnum = -1;
        return;
    }
    if (rh_proto_is_lost() || suppress_map_output()
        || (told_level.dnum == u.uz.dnum && told_level.dlevel == u.uz.dlevel))
        return;
    told_level = u.uz;
    a = args_new();
    add_str(a, "dungeon", svd.dungeons[u.uz.dnum].dname);
    add_int(a, "depth", depth(&u.uz));
    /* the status line names the plane the hero is on, so may we */
    if (In_endgame(&u.uz))
        add_str(a, "plane", Is_earthlevel(&u.uz) ? "earth"
                            : Is_airlevel(&u.uz) ? "air"
                            : Is_firelevel(&u.uz) ? "fire"
                            : Is_waterlevel(&u.uz) ? "water"
                            : "astral");
    rh_proto_send("win", "level", a);
}

/* The hero's progress, for achievements: the play mode, NetHack's own
   achievements in the order attained, the u.uevent milestones, the
   deepest level reached, the conducts, the role-play options and how the
   game ended.  Only what the game tells the player: #chronicle lists the
   achievements (but for the two spoilers, held back until the game is
   over: rh_progress.h), #overview and #conduct the rest.  Observation
   only: no RNG, no state change, no formatting buffers. */
static cJSON *
progress_json(void)
{
    cJSON *a = args_new(), *o;
    int i, ach;

    add_str(a, "mode", wizard ? "debug" : discover ? "explore" : "normal");
    o = cJSON_AddArrayToObject(a, "achieved");
    for (i = 0; i < N_ACH && u.uachieved[i]; i++) {
        /* the ranks are negated when the hero was female */
        ach = abs(u.uachieved[i]);
        if (rh_achievement_shown(ach, program_state.gameover))
            cJSON_AddItemToArray(o, cJSON_CreateNumber(ach));
    }
    o = cJSON_AddObjectToObject(a, "events");
    add_bool(o, "minor_oracle", u.uevent.minor_oracle);
    add_bool(o, "major_oracle", u.uevent.major_oracle);
    add_bool(o, "read_tribute", u.uevent.read_tribute);
    add_bool(o, "qcalled", u.uevent.qcalled);
    add_bool(o, "qexpelled", u.uevent.qexpelled);
    add_bool(o, "qcompleted", u.uevent.qcompleted);
    add_int(o, "uheard_tune", u.uevent.uheard_tune);
    add_bool(o, "uopened_dbridge", u.uevent.uopened_dbridge);
    add_bool(o, "invoked", u.uevent.invoked);
    add_bool(o, "gehennom_entered", u.uevent.gehennom_entered);
    add_int(o, "uhand_of_elbereth", u.uevent.uhand_of_elbereth);
    add_bool(o, "udemigod", u.uevent.udemigod);
    add_bool(o, "uvibrated", u.uevent.uvibrated);
    add_bool(o, "ascended", u.uevent.ascended);
    add_bool(o, "amulet_wish", u.uevent.amulet_wish);
    add_int(a, "deepest", deepest_lev_reached(FALSE));
    o = cJSON_AddObjectToObject(a, "conduct");
    add_int(o, "unvegetarian", u.uconduct.unvegetarian);
    add_int(o, "unvegan", u.uconduct.unvegan);
    add_int(o, "food", u.uconduct.food);
    add_int(o, "gnostic", u.uconduct.gnostic);
    add_int(o, "weaphit", u.uconduct.weaphit);
    add_int(o, "killer", u.uconduct.killer);
    add_int(o, "literate", u.uconduct.literate);
    add_int(o, "polypiles", u.uconduct.polypiles);
    add_int(o, "polyselfs", u.uconduct.polyselfs);
    add_int(o, "wishes", u.uconduct.wishes);
    add_int(o, "wisharti", u.uconduct.wisharti);
    add_int(o, "sokocheat", u.uconduct.sokocheat);
    add_int(o, "pets", u.uconduct.pets);
    o = cJSON_AddObjectToObject(a, "roleplay");
    add_bool(o, "blind", u.uroleplay.blind);
    add_bool(o, "nudist", u.uroleplay.nudist);
    add_bool(o, "deaf", u.uroleplay.deaf);
    add_bool(o, "pauper", u.uroleplay.pauper);
    add_bool(a, "gameover", program_state.gameover);
    add_str(a, "how",
            rh_end_how(program_state.gameover, svk.killer.name,
                       u.uevent.ascended, u.uhp > 0 || (Upolyd && u.mh > 0)));
    return a;
}

/* Before the engine waits for input: the progress notice, when it
   changed since the last one the client got. */
static void
progress_flush(void)
{
    cJSON *a;
    char *line;
    unsigned long long h;

    if (!program_state.in_moveloop && !program_state.gameover) {
        told_progress = 0;
        return;
    }
    if (rh_proto_is_lost() || suppress_map_output())
        return;
    a = progress_json();
    line = cJSON_PrintUnformatted(a);
    h = line ? rh_fnv1a64(line, strlen(line)) : 1;
    cJSON_free(line);
    if (h == told_progress) {
        cJSON_Delete(a);
        return;
    }
    told_progress = h;
    rh_proto_send("win", "progress", a);
}

/* every request of this file goes through here */
static cJSON *
request(const char *fn, cJSON *args)
{
    level_flush();
    inventory_flush();
    progress_flush();
    return rh_proto_request(fn, args);
}

static void
menu_reset(int w)
{
    if (!valid_win(w))
        return;
    free(windows[w].ids);
    windows[w].ids = (anything *) 0;
    windows[w].nitems = windows[w].maxitems = 0;
}

static void
menu_add(int w, const anything *id)
{
    struct rh_window *win;

    if (!valid_win(w))
        return;
    win = &windows[w];
    if (win->nitems == win->maxitems) {
        win->maxitems = win->maxitems ? win->maxitems * 2 : 16;
        win->ids = (anything *) realloc(win->ids,
                                        win->maxitems * sizeof (anything));
        if (!win->ids)
            panic("renethack: out of memory for menu items");
    }
    if (id)
        win->ids[win->nitems] = *id;
    else
        win->ids[win->nitems] = cg.zeroany;
    win->nitems++;
}

/* ---- handlers: one per shim call ---- */

typedef void (*rh_handler)(void *ret, va_list *ap);

static void
h_init_nhwindows(void *ret UNUSED, va_list *ap)
{
    char *catalog;

    (void) va_arg(*ap, int *);
    (void) va_arg(*ap, char **);
    /* the shim leaves this to the port; without it pline() falls back to
       raw_print() */
    iflags.window_inited = TRUE;
    /* early_init() has filled mons[] and objects[] by now, and o_init()
       has not shuffled appearances yet: the one moment the catalog is both
       complete and free of per-game secrets */
    catalog = rh_catalog_line();
    rh_proto_send_raw(catalog);
    free(catalog);
    rh_proto_send("win", "init_nhwindows", (cJSON *) 0);
}

static void
h_player_selection(void *ret UNUSED, va_list *ap UNUSED)
{
    /* role/race/gender/alignment normally arrive as options; anything left
       unset is asked through ordinary menus */
    if (!genl_player_setup(80)) {
        clearlocks();
        exit_nhwindows((char *) 0);
        nh_terminate(EXIT_SUCCESS);
    }
}

static void
h_askname(void *ret UNUSED, va_list *ap UNUSED)
{
    cJSON *r = request("askname", (cJSON *) 0);
    const char *name = r ? rh_reply_str(r, "text") : (const char *) 0;

    if (name && *name == '\033') {
        cJSON_Delete(r);
        clearlocks();
        exit_nhwindows((char *) 0);
        nh_terminate(EXIT_SUCCESS);
    }
    (void) strncpy(svp.plname, (name && *name) ? name : "Adventurer",
                   PL_NSIZ - 1);
    svp.plname[PL_NSIZ - 1] = '\0';
    cJSON_Delete(r);
}

static void
h_exit_nhwindows(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_str(a, "str", va_arg(*ap, const char *));
    rh_proto_send("win", "exit_nhwindows", a);
    rh_proto_flush();
}

static void
h_suspend_nhwindows(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_str(a, "str", va_arg(*ap, const char *));
    rh_proto_send("win", "suspend_nhwindows", a);
}

static void
h_resume_nhwindows(void *ret UNUSED, va_list *ap UNUSED)
{
    rh_proto_send("win", "resume_nhwindows", (cJSON *) 0);
}

static void
h_create_nhwindow(void *ret, va_list *ap)
{
    int type = va_arg(*ap, int), w;
    cJSON *a;

    for (w = 1; w < RH_MAX_WINDOWS; w++)
        if (!windows[w].type)
            break;
    if (w == RH_MAX_WINDOWS)
        panic("renethack: too many windows");
    windows[w].type = type;
    a = args_new();
    add_int(a, "win", w);
    cJSON_AddStringToObject(a, "type", wintype_name(type));
    rh_proto_send("win", "create_nhwindow", a);
    *(winid *) ret = w;
}

static void
h_clear_nhwindow(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "win", va_arg(*ap, int));
    rh_proto_send("win", "clear_nhwindow", a);
}

static void
h_display_nhwindow(void *ret UNUSED, va_list *ap)
{
    int w = va_arg(*ap, int), blocking = va_arg(*ap, int);
    cJSON *a = args_new();

    add_int(a, "win", w);
    /* tty waits on text and menu windows whatever the flag, and the core
       relies on it (#version, the key list, shop bills): so does the client */
    if (blocking
        || (valid_win(w)
            && (windows[w].type == NHW_TEXT || windows[w].type == NHW_MENU))) {
        /* "--More--" style pause: the client acknowledges */
        cJSON_Delete(request("display_nhwindow", a));
    } else {
        rh_proto_send("win", "display_nhwindow", a);
    }
}

static void
h_destroy_nhwindow(void *ret UNUSED, va_list *ap)
{
    int w = va_arg(*ap, int);
    cJSON *a = args_new();

    menu_reset(w);
    if (valid_win(w))
        windows[w].type = 0;
    add_int(a, "win", w);
    rh_proto_send("win", "destroy_nhwindow", a);
}

static void
h_curs(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "win", va_arg(*ap, int));
    add_int(a, "x", va_arg(*ap, int));
    add_int(a, "y", va_arg(*ap, int));
    rh_proto_send("win", "curs", a);
}

static void
h_putstr(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "win", va_arg(*ap, int));
    add_int(a, "attr", va_arg(*ap, int));
    add_str(a, "str", va_arg(*ap, const char *));
    rh_proto_send("win", "putstr", a);
}

static void
h_display_file(void *ret UNUSED, va_list *ap)
{
    const char *fname = va_arg(*ap, const char *);
    int complain = va_arg(*ap, int);
    char buf[BUFSZ], *nl;
    dlb *f = dlb_fopen(fname, "r");
    cJSON *a = args_new(), *lines;

    add_str(a, "name", fname);
    if (!f) {
        add_bool(a, "missing", 1);
        add_bool(a, "complain", complain);
        rh_proto_send("win", "display_file", a);
        return;
    }
    lines = cJSON_AddArrayToObject(a, "lines");
    while (dlb_fgets(buf, (int) sizeof buf, f)) {
        if ((nl = strchr(buf, '\n')) != 0)
            *nl = '\0';
        cJSON_AddItemToArray(lines, rh_json_string(buf));
    }
    (void) dlb_fclose(f);
    cJSON_Delete(request("display_file", a));
}

static void
h_start_menu(void *ret UNUSED, va_list *ap)
{
    int w = va_arg(*ap, int);
    unsigned long behavior = va_arg(*ap, unsigned long);
    cJSON *a = args_new();

    menu_reset(w);
    add_int(a, "win", w);
    add_int(a, "behavior", (long) behavior);
    rh_proto_send("win", "start_menu", a);
}

static void
h_add_menu(void *ret UNUSED, va_list *ap)
{
    int w = va_arg(*ap, int);
    const glyph_info *ginfo = va_arg(*ap, const glyph_info *);
    const anything *id = va_arg(*ap, const anything *);
    int ch = va_arg(*ap, int), gch = va_arg(*ap, int);
    int attr = va_arg(*ap, int), clr = va_arg(*ap, int);
    const char *str = va_arg(*ap, const char *);
    unsigned int itemflags = va_arg(*ap, unsigned int);
    cJSON *a = args_new();

    add_int(a, "win", w);
    add_int(a, "idx", valid_win(w) ? windows[w].nitems : -1);
    cJSON_AddItemToObject(a, "glyph",
                          (ginfo && ginfo->glyph != NO_GLYPH) ? glyph_json(ginfo)
                                                        : cJSON_CreateNull());
    add_bool(a, "selectable", id && id->a_void);
    add_int(a, "ch", ch);
    add_int(a, "gch", gch);
    add_int(a, "attr", attr);
    add_int(a, "clr", clr);
    add_str(a, "str", str);
    add_bool(a, "preselected", (itemflags & MENU_ITEMFLAGS_SELECTED) != 0);
    /* bulk select/invert ('.', ',', '@') must not turn such items on
       ("Auto-select every relevant item", "All types") */
    add_bool(a, "skipinvert", (itemflags & MENU_ITEMFLAGS_SKIPINVERT) != 0);
    menu_add(w, id);
    rh_proto_send("win", "add_menu", a);
}

static void
h_end_menu(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "win", va_arg(*ap, int));
    add_str(a, "prompt", va_arg(*ap, const char *));
    rh_proto_send("win", "end_menu", a);
}

/* reply: {"items":[[idx,count],...]} or {"cancel":true}.  window.txt: each
   item at most once, count -1 ("no count given") or a count >= 1, and a
   pick-one menu takes at most one item.  Anything else would reach the core
   as nonsense (a negative count panics splitobj()), so it is a protocol
   violation, handled like a lost client. */
static void
h_select_menu(void *ret, va_list *ap)
{
    int w = va_arg(*ap, int), how = va_arg(*ap, int);
    menu_item **menu_list = va_arg(*ap, menu_item **);
    cJSON *a = args_new(), *r, *items;
    menu_item *mi = (menu_item *) 0;
    int *picked = (int *) 0;
    int n = 0, k, j, idx;
    double count;
    const char *why = (const char *) 0;

    *menu_list = (menu_item *) 0;
    add_int(a, "win", w);
    add_int(a, "how", how);
    r = request("select_menu", a);
    if (!r || rh_reply_has(r, "cancel")) {
        cJSON_Delete(r);
        *(int *) ret = -1;
        return;
    }
    items = cJSON_GetObjectItemCaseSensitive(r, "items");
    if (how != PICK_NONE && cJSON_IsArray(items))
        n = cJSON_GetArraySize(items);
    if (how == PICK_ONE && n > 1)
        why = "select_menu reply picks more than one item from a pick-one"
              " menu";
    if (n > 0 && !why) {
        mi = (menu_item *) alloc((unsigned) (n * sizeof (menu_item)));
        picked = (int *) alloc((unsigned) (n * sizeof (int)));
    }
    for (k = 0; k < n && !why; k++) {
        const cJSON *pair = cJSON_GetArrayItem(items, k), *jidx, *jcount;

        if (!cJSON_IsArray(pair) || cJSON_GetArraySize(pair) != 2
            || !cJSON_IsNumber(jidx = cJSON_GetArrayItem(pair, 0))
            || !cJSON_IsNumber(jcount = cJSON_GetArrayItem(pair, 1))) {
            why = "select_menu reply item is not an [index, count] pair";
            break;
        }
        /* range checks come before any cast: out-of-range casts are UB */
        if (!valid_win(w) || !(jidx->valuedouble >= 0.0
                               && jidx->valuedouble < windows[w].nitems)
            || !windows[w].ids[(int) jidx->valuedouble].a_void) {
            why = "select_menu reply names an item that is not selectable";
            break;
        }
        idx = (int) jidx->valuedouble;
        for (j = 0; j < k; j++)
            if (picked[j] == idx)
                why = "select_menu reply names an item twice";
        count = jcount->valuedouble;
        if (!why && count != -1.0
            && !(count >= 1.0 && count <= 2147483647.0
                 && count == (double) (long) count))
            why = "select_menu reply has an item count that is neither -1"
                  " nor a positive whole number";
        if (why)
            break;
        picked[k] = idx;
        mi[k].item = windows[w].ids[idx];
        mi[k].count = (long) count;
        mi[k].itemflags = MENU_ITEMFLAGS_NONE;
    }
    free(picked);
    cJSON_Delete(r);
    if (why) {
        free(mi);
        rh_proto_violation(why);
        *(int *) ret = -1;
        return;
    }
    *menu_list = mi;
    *(int *) ret = n;
}

/* window.txt: the answer is `let' (the item was picked), '\033' (cancel)
   or '\0' (nothing picked); any other key means nothing picked. */
static void
h_message_menu(void *ret, va_list *ap)
{
    int let = va_arg(*ap, int);
    cJSON *a = args_new(), *r;
    long ch;

    add_int(a, "let", let);
    add_int(a, "how", va_arg(*ap, int));
    add_str(a, "mesg", va_arg(*ap, const char *));
    r = request("message_menu", a);
    ch = r ? rh_reply_int(r, "ch", '\033') : '\033';
    cJSON_Delete(r);
    if (ch != let && ch != '\033')
        ch = '\0';
    *(char *) ret = (char) ch;
}

static void
h_mark_synch(void *ret UNUSED, va_list *ap UNUSED)
{
    rh_proto_send("win", "mark_synch", (cJSON *) 0);
    rh_proto_flush();
}

static void
h_wait_synch(void *ret UNUSED, va_list *ap UNUSED)
{
    rh_proto_send("win", "wait_synch", (cJSON *) 0);
    rh_proto_flush();
}

static void
h_cliparound(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "x", va_arg(*ap, int));
    add_int(a, "y", va_arg(*ap, int));
    rh_proto_send("win", "cliparound", a);
}

static void
h_print_glyph(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "win", va_arg(*ap, int));
    add_int(a, "x", va_arg(*ap, int));
    add_int(a, "y", va_arg(*ap, int));
    cJSON_AddItemToObject(a, "g", glyph_json(va_arg(*ap, const glyph_info *)));
    cJSON_AddItemToObject(a, "bk",
                          glyph_json(va_arg(*ap, const glyph_info *)));
    rh_proto_send("win", "print_glyph", a);
}

static void
raw_print_common(va_list *ap, int bold)
{
    cJSON *a = args_new();

    add_str(a, "str", va_arg(*ap, const char *));
    add_bool(a, "bold", bold);
    rh_proto_send("win", "raw_print", a);
    rh_proto_flush();
}

static void
h_raw_print(void *ret UNUSED, va_list *ap)
{
    raw_print_common(ap, 0);
}

static void
h_raw_print_bold(void *ret UNUSED, va_list *ap)
{
    raw_print_common(ap, 1);
}

static void
h_nhgetch(void *ret, va_list *ap UNUSED)
{
    cJSON *r = request("nhgetch", (cJSON *) 0);

    *(int *) ret = r ? (int) rh_reply_int(r, "key", '\033') : EOF;
    cJSON_Delete(r);
}

/* reply: {"key":k} for a keystroke or {"x":..,"y":..,"mod":..} for a click */
static void
h_nh_poskey(void *ret, va_list *ap)
{
    coordxy *x = va_arg(*ap, coordxy *), *y = va_arg(*ap, coordxy *);
    int *mod = va_arg(*ap, int *);
    cJSON *a = (cJSON *) 0, *r;

    /* getpos() moves a cursor over the map with these keys: the client
       says so (the cursor is not the hero, the prompt is "pick a spot") */
    if (gg.getposx) {
        a = args_new();
        add_bool(a, "getpos", 1);
    }
    r = request("nh_poskey", a);

    if (!r) {
        *(int *) ret = EOF;
        return;
    }
    if (rh_reply_has(r, "x")) {
        *x = (coordxy) rh_reply_int(r, "x", 0);
        *y = (coordxy) rh_reply_int(r, "y", 0);
        *mod = (int) rh_reply_int(r, "mod", CLICK_1);
        *(int *) ret = 0;
    } else {
        *(int *) ret = (int) rh_reply_int(r, "key", '\033');
    }
    cJSON_Delete(r);
}

static void
h_nhbell(void *ret UNUSED, va_list *ap UNUSED)
{
    rh_proto_send("win", "nhbell", (cJSON *) 0);
}

static void
h_doprev_message(void *ret, va_list *ap UNUSED)
{
    rh_proto_send("win", "doprev_message", (cJSON *) 0);
    *(int *) ret = 0;
}

/* window.txt / tty: the answer is one of `choices'; ESC means 'q' when
   that is offered, else 'n', else the default.  With no choices any key
   goes back as it is.  A reply outside the choices is a protocol violation
   (the core would report "Program in disorder"). */
static char
yn_escape(const char *choices, char def)
{
    if (strchr(choices, 'q'))
        return 'q';
    if (strchr(choices, 'n'))
        return 'n';
    return def;
}

static void
h_yn_function(void *ret, va_list *ap)
{
    const char *query = va_arg(*ap, const char *);
    const char *choices = va_arg(*ap, const char *);
    char def = (char) va_arg(*ap, int);
    cJSON *a = args_new(), *r;
    long ch;
    int listed = choices && *choices;

    add_str(a, "query", query);
    add_str(a, "choices", choices);
    add_int(a, "default", def);
    r = request("yn_function", a);
    ch = r ? rh_reply_int(r, "ch", '\033') : '\033';
    cJSON_Delete(r);
    if (listed && ch == '\033' && !strchr(choices, '\033')) {
        ch = yn_escape(choices, def);
    } else if (ch < 0 || ch > 255
               || (listed && ch && !strchr(choices, (int) ch))) {
        rh_proto_violation("yn_function reply is not one of the choices");
        ch = listed ? yn_escape(choices, def) : '\033';
    }
    *(char *) ret = (char) ch;
}

static void
h_getlin(void *ret UNUSED, va_list *ap)
{
    const char *query = va_arg(*ap, const char *);
    char *bufp = va_arg(*ap, char *);
    cJSON *a = args_new(), *r;
    const char *text;

    add_str(a, "query", query);
    r = request("getlin", a);
    text = r ? rh_reply_str(r, "text") : (const char *) 0;
    (void) strncpy(bufp, text ? text : "\033", BUFSZ - 1);
    bufp[BUFSZ - 1] = '\0';
    cJSON_Delete(r);
}

static void
h_get_ext_cmd(void *ret, va_list *ap UNUSED)
{
    cJSON *r = request("get_ext_cmd", (cJSON *) 0);
    const char *cmd = r ? rh_reply_str(r, "cmd") : (const char *) 0;
    struct ext_func_tab *e;
    int i, found = -1;

    for (i = 0; cmd && (e = extcmds_getentry(i)) != 0 && e->ef_txt; i++)
        if (!strcmp(e->ef_txt, cmd)) {
            found = i;
            break;
        }
    *(int *) ret = found;
    cJSON_Delete(r);
}

/* after the number_pad option changes; reset_commands() has run, so the
   current direction keys (swap_yz, phone layout...) go along */
static void
h_number_pad(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "state", va_arg(*ap, int));
    add_str(a, "dirchars", gc.Cmd.dirchars);
    rh_proto_send("win", "number_pad", a);
}

static void
h_delay_output(void *ret UNUSED, va_list *ap UNUSED)
{
    rh_proto_send("win", "delay_output", (cJSON *) 0);
    rh_proto_flush();
}

static void
h_preference_update(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_str(a, "pref", va_arg(*ap, const char *));
    rh_proto_send("win", "preference_update", a);
}

static void
h_getmsghistory(void *ret, va_list *ap)
{
    (void) va_arg(*ap, int);
    *(char **) ret = (char *) 0; /* the client keeps its own log */
}

static void
h_putmsghistory(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_str(a, "msg", va_arg(*ap, const char *));
    add_bool(a, "restoring", va_arg(*ap, int));
    rh_proto_send("win", "putmsghistory", a);
}

static void
h_status_init(void *ret UNUSED, va_list *ap UNUSED)
{
    rh_proto_send("win", "status_init", (cJSON *) 0);
}

static void
h_status_update(void *ret UNUSED, va_list *ap)
{
    int fld = va_arg(*ap, int);
    genericptr_t ptr = va_arg(*ap, genericptr_t);
    int chg = va_arg(*ap, int), percent = va_arg(*ap, int);
    int color = va_arg(*ap, int);
    cJSON *a = args_new();

    (void) va_arg(*ap, unsigned long *); /* colormasks: unused for now */
    if (fld == BL_FLUSH || fld == BL_RESET) {
        cJSON_AddStringToObject(a, "field",
                                fld == BL_FLUSH ? "flush" : "reset");
    } else if (fld == BL_CONDITION) {
        cJSON_AddStringToObject(a, "field", "condition");
        add_int(a, "conds", ptr ? (long) *(unsigned long *) ptr : 0L);
    } else if (fld >= 0 && fld < MAXBLSTATS) {
        cJSON_AddStringToObject(a, "field", status_names[fld]);
        add_str(a, "value", (const char *) ptr);
        add_int(a, "chg", chg);
        add_int(a, "percent", percent);
        add_int(a, "color", color);
    } else {
        cJSON_Delete(a);
        return;
    }
    rh_proto_send("win", "status_update", a);
}

static void
h_update_inventory(void *ret UNUSED, va_list *ap)
{
    cJSON *a = args_new();

    add_int(a, "arg", va_arg(*ap, int));
    rh_proto_send("win", "update_inventory", a);
    inventory_dirty = TRUE;
}

static void
h_ignore(void *ret UNUSED, va_list *ap UNUSED)
{
}

static const struct {
    const char *name;
    rh_handler fn;
} dispatch[] = {
    { "shim_init_nhwindows", h_init_nhwindows },
    { "shim_player_selection", h_player_selection },
    { "shim_askname", h_askname },
    { "shim_get_nh_event", h_ignore },
    { "shim_exit_nhwindows", h_exit_nhwindows },
    { "shim_suspend_nhwindows", h_suspend_nhwindows },
    { "shim_resume_nhwindows", h_resume_nhwindows },
    { "shim_create_nhwindow", h_create_nhwindow },
    { "shim_clear_nhwindow", h_clear_nhwindow },
    { "shim_display_nhwindow", h_display_nhwindow },
    { "shim_destroy_nhwindow", h_destroy_nhwindow },
    { "shim_curs", h_curs },
    { "shim_putstr", h_putstr },
    { "shim_display_file", h_display_file },
    { "shim_start_menu", h_start_menu },
    { "shim_add_menu", h_add_menu },
    { "shim_end_menu", h_end_menu },
    { "shim_select_menu", h_select_menu },
    { "shim_message_menu", h_message_menu },
    { "shim_mark_synch", h_mark_synch },
    { "shim_wait_synch", h_wait_synch },
    { "shim_cliparound", h_cliparound },
    { "shim_update_positionbar", h_ignore },
    { "shim_print_glyph", h_print_glyph },
    { "shim_raw_print", h_raw_print },
    { "shim_raw_print_bold", h_raw_print_bold },
    { "shim_nhgetch", h_nhgetch },
    { "shim_nh_poskey", h_nh_poskey },
    { "shim_nhbell", h_nhbell },
    { "shim_doprev_message", h_doprev_message },
    { "shim_yn_function", h_yn_function },
    { "shim_getlin", h_getlin },
    { "shim_get_ext_cmd", h_get_ext_cmd },
    { "shim_number_pad", h_number_pad },
    { "shim_delay_output", h_delay_output },
    { "shim_preference_update", h_preference_update },
    { "shim_getmsghistory", h_getmsghistory },
    { "shim_putmsghistory", h_putmsghistory },
    { "shim_status_init", h_status_init },
    { "shim_status_update", h_status_update },
    { "shim_update_inventory", h_update_inventory },
    { "shim_ctrl_nhwindow", h_ignore }, /* NULL reply is the default */
};

void
rh_bridge_callback(const char *name, void *ret_ptr, const char *fmt, ...)
{
    va_list ap;
    size_t i;

    nhUse(fmt);
    for (i = 0; i < SIZE(dispatch); i++)
        if (!strcmp(dispatch[i].name, name)) {
            va_start(ap, fmt);
            (*dispatch[i].fn)(ret_ptr, &ap);
            va_end(ap);
            return;
        }
    fprintf(stderr, "renethack: unhandled shim call %s\n", name);
}

/* the client vanished or broke the protocol: behave like a terminal hangup,
   so the core saves the game at its next safe point */
static void
client_lost(void)
{
    hangup(0);
}

void
rh_bridge_start(void)
{
    char version[32];
    cJSON *a = args_new();

    Snprintf(version, sizeof version, "%d.%d.%d", VERSION_MAJOR,
             VERSION_MINOR, PATCHLEVEL);
    add_int(a, "protocol", RH_PROTOCOL_VERSION);
    cJSON_AddStringToObject(a, "engine", version);
    cJSON_AddStringToObject(a, "patchset", RH_PATCHSET);
    rh_proto_send("hello", (const char *) 0, a);
    rh_proto_flush();
    rh_proto_set_lost_handler(client_lost);
}

void
rh_bridge_atexit(void)
{
    rh_proto_send("bye", (const char *) 0, (cJSON *) 0);
    rh_proto_flush();
}
