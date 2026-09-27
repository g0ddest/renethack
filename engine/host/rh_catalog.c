/* renethack engine host: the catalog, sent once from init_nhwindows.
 * By then early_init() has filled mons[]/objects[], and o_init() has not
 * yet shuffled object appearances.  Object tiles are described by
 * appearance only -- the true object type never leaves the engine. */
#include "hack.h"
#include "func_tab.h"
#include "rh_proto.h"
#include "rh_bridge.h"

/* defined in the generated src/tile.c (no header declares them) */
extern int maxmontile, maxobjtile, maxothtile;
extern glyph_map glyphmap[MAX_GLYPH];

static const char *
size_name(int msize)
{
    switch (msize) {
    case MZ_TINY: return "tiny";
    case MZ_SMALL: return "small";
    case MZ_MEDIUM: return "medium";
    case MZ_LARGE: return "large";
    case MZ_HUGE: return "huge";
    default: return "gigantic";
    }
}

static cJSON *
body_flags(const struct permonst *pm)
{
    static const struct {
        unsigned long bit;
        int which; /* 1 = mflags1, 2 = mflags2 */
        const char *name;
    } body_bits[] = {
        { M1_FLY, 1, "fly" },           { M1_SWIM, 1, "swim" },
        { M1_AMORPHOUS, 1, "amorphous" }, { M1_WALLWALK, 1, "wallwalk" },
        { M1_NOEYES, 1, "noeyes" },     { M1_NOHANDS, 1, "nohands" },
        { M1_NOLIMBS, 1, "nolimbs" },   { M1_NOHEAD, 1, "nohead" },
        { M1_HUMANOID, 1, "humanoid" }, { M1_ANIMAL, 1, "animal" },
        { M1_SLITHY, 1, "slithy" },     { M1_UNSOLID, 1, "unsolid" },
        { M1_THICK_HIDE, 1, "thick_hide" },
        { M2_UNDEAD, 2, "undead" },     { M2_DEMON, 2, "demon" },
        { M2_GIANT, 2, "giant" },
    };
    cJSON *arr = cJSON_CreateArray();
    size_t i;

    for (i = 0; i < SIZE(body_bits); i++) {
        unsigned long bits = body_bits[i].which == 1 ? pm->mflags1
                                                     : pm->mflags2;

        /* some M1_ values are composite masks (M1_NOLIMBS includes
           M1_NOHANDS), so require every bit */
        if ((bits & body_bits[i].bit) == body_bits[i].bit)
            cJSON_AddItemToArray(arr, cJSON_CreateString(body_bits[i].name));
    }
    return arr;
}

static cJSON *
monsters_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *m;
    int i;
    char sym[2];

    for (i = LOW_PM; i < NUMMONS; i++) {
        const struct permonst *pm = &mons[i];

        m = cJSON_CreateObject();
        cJSON_AddNumberToObject(m, "idx", i);
        cJSON_AddItemToObject(m, "name", rh_json_string(pm->pmnames[NEUTRAL]));
        cJSON_AddItemToObject(m, "male", rh_json_string(pm->pmnames[MALE]));
        cJSON_AddItemToObject(m, "female",
                              rh_json_string(pm->pmnames[FEMALE]));
        sym[0] = def_monsyms[(int) pm->mlet].sym;
        sym[1] = '\0';
        cJSON_AddStringToObject(m, "class", sym);
        cJSON_AddItemToObject(m, "class_name",
                              rh_json_string(def_monsyms[(int) pm->mlet]
                                                 .explain));
        cJSON_AddStringToObject(m, "size", size_name(pm->msize));
        cJSON_AddNumberToObject(m, "color", pm->mcolor);
        cJSON_AddNumberToObject(m, "light", emits_light(pm));
        cJSON_AddItemToObject(m, "body", body_flags(pm));
        cJSON_AddNumberToObject(m, "tile_male",
                                glyphmap[GLYPH_MON_MALE_OFF + i].tileidx);
        cJSON_AddNumberToObject(m, "tile_female",
                                glyphmap[GLYPH_MON_FEM_OFF + i].tileidx);
        cJSON_AddItemToArray(arr, m);
    }
    return arr;
}

/* object tile -> the tile that stands for its (class, appearance).  NetHack
   gives look-alike objects it never shuffles (sack / bag of holding, oil /
   magic lamp, the gray stones, glass vs gems of one color) tiles of their
   own; sending those would reveal the object type.  So every object goes out
   with the first tile of its look-alike group, and the catalog lists each
   appearance once.  Built with the catalog, before o_init() shuffles. */
static int *appearance_tile;
static int appearance_tile_len;

int
rh_object_appearance_tile(int tile)
{
    if (appearance_tile && tile >= 0 && tile < appearance_tile_len
        && appearance_tile[tile] >= 0)
        return appearance_tile[tile];
    return tile;
}

static const char *
object_appearance(int i)
{
    return obj_descr[i].oc_descr ? obj_descr[i].oc_descr
                                 : obj_descr[i].oc_name;
}

static cJSON *
object_tiles_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *t;
    int i, j, tile;
    char sym[2];

    free(appearance_tile);
    appearance_tile_len = maxothtile + 1;
    appearance_tile = (int *) alloc(
        (unsigned) (appearance_tile_len * sizeof (int)));
    for (i = 0; i < appearance_tile_len; i++)
        appearance_tile[i] = -1;
    for (i = 0; i < NUM_OBJECTS; i++) {
        /* before o_init, object i shows its own appearance on its own tile */
        const char *appearance = object_appearance(i);
        int cls = objects[i].oc_class;

        if (!appearance)
            continue;
        tile = glyphmap[GLYPH_OBJ_OFF + i].tileidx;
        for (j = 0; j < i; j++)
            if (objects[j].oc_class == cls && object_appearance(j)
                && !strcmp(object_appearance(j), appearance))
                break;
        if (j < i) {
            appearance_tile[tile] =
                appearance_tile[glyphmap[GLYPH_OBJ_OFF + j].tileidx];
            continue;
        }
        appearance_tile[tile] = tile;
        t = cJSON_CreateObject();
        cJSON_AddNumberToObject(t, "tile", tile);
        sym[0] = def_oc_syms[cls].sym;
        sym[1] = '\0';
        cJSON_AddStringToObject(t, "class", sym);
        cJSON_AddItemToObject(t, "class_name",
                              rh_json_string(def_oc_syms[cls].name));
        cJSON_AddItemToObject(t, "appearance", rh_json_string(appearance));
        cJSON_AddItemToArray(arr, t);
    }
    return arr;
}

/* symbolic names of the map symbols ("S_vwall"): the explanations in
   defsyms[] are not unique ("wall" for every wall), so clients classify map
   features by these */
static const struct {
    int idx;
    const char *sym;
} cmap_syms[] = {
#define DUMP_ENUMS_PCHAR
#include "defsym.h"
#undef DUMP_ENUMS_PCHAR
};

static const char *
cmap_sym(int i)
{
    size_t k;

    for (k = 0; k < SIZE(cmap_syms); k++)
        if (cmap_syms[k].idx == i)
            return cmap_syms[k].sym;
    return "";
}

static cJSON *
cmap_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *c;
    int i;

    for (i = 0; i < MAXPCHARS; i++) {
        c = cJSON_CreateObject();
        cJSON_AddNumberToObject(c, "idx", i);
        cJSON_AddItemToObject(c, "name", rh_json_string(defsyms[i].explanation));
        cJSON_AddStringToObject(c, "sym", cmap_sym(i));
        cJSON_AddNumberToObject(c, "ch", defsyms[i].sym);
        cJSON_AddNumberToObject(c, "color", defsyms[i].color);
        cJSON_AddItemToArray(arr, c);
    }
    return arr;
}

static cJSON *
roles_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *r, *combos, *combo, *list;
    int ri, ra, g, al;

    for (ri = 0; roles[ri].name.m; ri++) {
        r = cJSON_CreateObject();
        cJSON_AddNumberToObject(r, "idx", ri);
        cJSON_AddItemToObject(r, "name", rh_json_string(roles[ri].name.m));
        cJSON_AddItemToObject(r, "name_female",
                              rh_json_string(roles[ri].name.f));
        cJSON_AddItemToObject(r, "code", rh_json_string(roles[ri].filecode));
        combos = cJSON_AddArrayToObject(r, "combos");
        for (ra = 0; races[ra].noun; ra++) {
            if (!validrace(ri, ra))
                continue;
            combo = cJSON_CreateObject();
            cJSON_AddNumberToObject(combo, "race", ra);
            list = cJSON_AddArrayToObject(combo, "genders");
            for (g = 0; g < ROLE_GENDERS; g++)
                if (validgend(ri, ra, g))
                    cJSON_AddItemToArray(list, cJSON_CreateNumber(g));
            list = cJSON_AddArrayToObject(combo, "aligns");
            for (al = 0; al < ROLE_ALIGNS; al++)
                if (validalign(ri, ra, al))
                    cJSON_AddItemToArray(list, cJSON_CreateNumber(al));
            cJSON_AddItemToArray(combos, combo);
        }
        cJSON_AddItemToArray(arr, r);
    }
    return arr;
}

static cJSON *
races_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *r;
    int i;

    for (i = 0; races[i].noun; i++) {
        r = cJSON_CreateObject();
        cJSON_AddNumberToObject(r, "idx", i);
        cJSON_AddItemToObject(r, "noun", rh_json_string(races[i].noun));
        cJSON_AddItemToObject(r, "adj", rh_json_string(races[i].adj));
        cJSON_AddItemToObject(r, "code", rh_json_string(races[i].filecode));
        cJSON_AddItemToArray(arr, r);
    }
    return arr;
}

static cJSON *
genders_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *r;
    int i;

    for (i = 0; i < ROLE_GENDERS; i++) {
        r = cJSON_CreateObject();
        cJSON_AddNumberToObject(r, "idx", i);
        cJSON_AddItemToObject(r, "adj", rh_json_string(genders[i].adj));
        cJSON_AddItemToObject(r, "code", rh_json_string(genders[i].filecode));
        cJSON_AddItemToArray(arr, r);
    }
    return arr;
}

static cJSON *
aligns_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *r;
    int i;

    for (i = 0; i < ROLE_ALIGNS; i++) {
        r = cJSON_CreateObject();
        cJSON_AddNumberToObject(r, "idx", i);
        cJSON_AddItemToObject(r, "adj", rh_json_string(aligns[i].adj));
        cJSON_AddItemToObject(r, "code", rh_json_string(aligns[i].filecode));
        cJSON_AddItemToArray(arr, r);
    }
    return arr;
}

static cJSON *
glyph_offsets_json(void)
{
    cJSON *o = cJSON_CreateObject();

    cJSON_AddNumberToObject(o, "max", MAX_GLYPH);
    cJSON_AddNumberToObject(o, "mon", GLYPH_MON_OFF);
    cJSON_AddNumberToObject(o, "pet", GLYPH_PET_OFF);
    cJSON_AddNumberToObject(o, "invisible", GLYPH_INVIS_OFF);
    cJSON_AddNumberToObject(o, "detect", GLYPH_DETECT_OFF);
    cJSON_AddNumberToObject(o, "body", GLYPH_BODY_OFF);
    cJSON_AddNumberToObject(o, "ridden", GLYPH_RIDDEN_OFF);
    cJSON_AddNumberToObject(o, "obj", GLYPH_OBJ_OFF);
    cJSON_AddNumberToObject(o, "cmap", GLYPH_CMAP_OFF);
    cJSON_AddNumberToObject(o, "zap", GLYPH_ZAP_OFF);
    cJSON_AddNumberToObject(o, "swallow", GLYPH_SWALLOW_OFF);
    cJSON_AddNumberToObject(o, "explode", GLYPH_EXPLODE_OFF);
    cJSON_AddNumberToObject(o, "warning", GLYPH_WARNING_OFF);
    cJSON_AddNumberToObject(o, "statue", GLYPH_STATUE_OFF);
    cJSON_AddNumberToObject(o, "unexplored", GLYPH_UNEXPLORED_OFF);
    cJSON_AddNumberToObject(o, "nothing", GLYPH_NOTHING_OFF);
    return o;
}

/* extended commands a player can type: no internal, wizard-mode or
   unavailable ones */
static cJSON *
extcmds_json(void)
{
    static const struct {
        unsigned bit;
        const char *name;
    } flag_names[] = {
        { AUTOCOMPLETE, "autocomplete" }, { GENERALCMD, "general" },
        { PREFIXCMD, "prefix" },          { MOVEMENTCMD, "movement" },
    };
    cJSON *arr = cJSON_CreateArray(), *e, *fl;
    const struct ext_func_tab *ef;
    size_t k;
    int i;

    for (i = 0; (ef = extcmds_getentry(i)) != 0 && ef->ef_txt; i++) {
        if (ef->flags & (INTERNALCMD | WIZMODECMD | CMD_NOT_AVAILABLE))
            continue;
        e = cJSON_CreateObject();
        cJSON_AddItemToObject(e, "name", rh_json_string(ef->ef_txt));
        cJSON_AddItemToObject(e, "desc", rh_json_string(ef->ef_desc));
        cJSON_AddNumberToObject(e, "key", ef->key);
        fl = cJSON_AddArrayToObject(e, "flags");
        for (k = 0; k < SIZE(flag_names); k++)
            if (ef->flags & flag_names[k].bit)
                cJSON_AddItemToArray(fl,
                                     cJSON_CreateString(flag_names[k].name));
        cJSON_AddItemToArray(arr, e);
    }
    return arr;
}

/* status conditions: the bits of status_update's "conds" and their names */
static cJSON *
conditions_json(void)
{
    cJSON *arr = cJSON_CreateArray(), *c;
    const char *shortest;
    int i, t;

    for (i = 0; i < CONDITION_COUNT; i++) {
        c = cJSON_CreateObject();
        cJSON_AddNumberToObject(c, "mask", (double) conditions[i].mask);
        cJSON_AddItemToObject(c, "name", rh_json_string(conditions[i].text[0]));
        shortest = conditions[i].text[0];
        for (t = 1; t < 3; t++)
            if (conditions[i].text[t]
                && strlen(conditions[i].text[t]) < strlen(shortest))
                shortest = conditions[i].text[t];
        cJSON_AddItemToObject(c, "short", rh_json_string(shortest));
        cJSON_AddItemToArray(arr, c);
    }
    return arr;
}

char *
rh_catalog_line(void)
{
    cJSON *msg = cJSON_CreateObject(), *a = cJSON_CreateObject(), *tiles;
    char *line;

    cJSON_AddStringToObject(msg, "t", "catalog");
    cJSON_AddItemToObject(msg, "a", a);
    cJSON_AddItemToObject(a, "glyphs", glyph_offsets_json());
    tiles = cJSON_AddObjectToObject(a, "tiles");
    cJSON_AddNumberToObject(tiles, "last_monster", maxmontile);
    cJSON_AddNumberToObject(tiles, "last_object", maxobjtile);
    cJSON_AddNumberToObject(tiles, "last_other", maxothtile);
    cJSON_AddItemToObject(a, "monsters", monsters_json());
    cJSON_AddItemToObject(a, "object_tiles", object_tiles_json());
    cJSON_AddItemToObject(a, "cmap", cmap_json());
    cJSON_AddItemToObject(a, "roles", roles_json());
    cJSON_AddItemToObject(a, "races", races_json());
    cJSON_AddItemToObject(a, "genders", genders_json());
    cJSON_AddItemToObject(a, "aligns", aligns_json());
    cJSON_AddItemToObject(a, "extcmds", extcmds_json());
    cJSON_AddItemToObject(a, "conditions", conditions_json());
    line = cJSON_PrintUnformatted(msg);
    cJSON_Delete(msg);
    if (!line)
        panic("renethack: cannot serialize the catalog");
    return line;
}
