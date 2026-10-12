/* renethack engine host: nhmain() for Windows.
 * The unix build links NetHack's sys/libnh/libnhmain.c; on Windows NetHack
 * builds its WIN32 configuration (windconf.h, sys/windows/windsys.c), whose
 * own main (windmain.c) runs a console or a GUI.  This is libnhmain.c's
 * sequence on Windows instead: the shim window port, every file relative to
 * the working directory (the client's playground), lock slots "alock".. as
 * on unix (MAXPLAYERS in sysconf), so the client finds an interrupted game
 * the same way.  Nothing here changes how the game plays. */
#include "win32api.h" /* windows.h, as NetHack's Windows files have it */
#include "hack.h"
#include "dlb.h"
#include <fcntl.h>
#include <io.h>

int nhmain(int, char **);

/* windmain.c's globals that the core and windsys.c read */
int GUILaunched = 0;
boolean getreturn_enabled = FALSE; /* never wait for a key at exit */

static void process_options(int, char **);
static void rh_getlock(void);
static void wd_message(void);

/* the number of lock slots, sysconf's MAXPLAYERS (gl.locknum on unix);
   0: lock by the character's name */
static int locknum;

int
nhmain(int argc, char *argv[])
{
    NHFILE *nhfp;
    boolean resuming = FALSE; /* assume new game */
    boolean plsel_once = FALSE;

    /* no Windows Error Reporting dialog for a crash: the client sees the
       engine end and recovers the game */
    (void) SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);

    early_init(argc, argv);
    gh.hname = "nh-engine";
    svh.hackpid = (int) GetCurrentProcessId();
    choose_windows(DEFAULT_WINDOW_SYS);
    /* nhdat, not windconf.h's nhdat<version>: the playground has one data
       file name on every platform */
    Strcpy(dlbfilename, "nhdat");

    initoptions();
    u.uhp = 1; /* prevent RIP on early quits */
    program_state.preserve_locks = 1;
    process_options(argc, argv);
    init_nhwindows(&argc, argv);

    set_playmode(); /* sets plname to "wizard" for wizard mode */
    plnamesuffix();
    if (wizard)
        locknum = 0; /* lock by the character's name */

    dlb_init(); /* must be before newgame() */
    vision_init();
    init_sound_disp_gamewindows();

 attempt_restore:
    if (*svp.plname) {
        rh_getlock();
        program_state.preserve_locks = 0; /* after rh_getlock() */
    }

    if (*svp.plname && (nhfp = restore_saved_game()) != 0) {
        pline("Restoring save file...");
        mark_synch(); /* flush output */
        if (dorecover(nhfp)) {
            resuming = TRUE; /* not starting new game */
            wd_message();
            if (discover || wizard) {
                if (y_n("Do you want to keep the save file?") == 'n')
                    (void) delete_savefile();
            }
        }
    }

    if (!resuming) {
        boolean neednewlock = (!*svp.plname);

        if (!iflags.renameinprogress || iflags.defer_plname || neednewlock) {
            if (!plsel_once)
                player_selection();
            plsel_once = TRUE;
            if (neednewlock && *svp.plname)
                goto attempt_restore;
            if (iflags.renameinprogress) {
                if (!locknum) {
                    delete_levelfile(0); /* remove empty lock file */
                    rh_getlock();
                }
                goto attempt_restore;
            }
        }
        newgame();
        wd_message();
    }

    /* moveloop() never returns but isn't flagged NORETURN */
    moveloop(resuming);
    exit(EXIT_SUCCESS);
    /*NOTREACHED*/
    return 0;
}

/* libnhmain.c's: -D, -X, -u name, -p role, -r race, -@; and MAXPLAYERS */
static void
process_options(int argc, char *argv[])
{
    int i;

    while (argc > 1 && argv[1][0] == '-') {
        argv++;
        argc--;
        switch (argv[0][1]) {
        case 'D':
            wizard = TRUE, discover = FALSE;
            break;
        case 'X':
            discover = TRUE, wizard = FALSE;
            break;
        case 'u':
            if (argv[0][2]) {
                (void) strncpy(svp.plname, argv[0] + 2, sizeof svp.plname - 1);
            } else if (argc > 1) {
                argc--;
                argv++;
                (void) strncpy(svp.plname, argv[0], sizeof svp.plname - 1);
            }
            gp.plnamelen = 0;
            break;
        case 'p':
            if (argv[0][2] && (i = str2role(&argv[0][2])) >= 0)
                flags.initrole = i;
            break;
        case 'r':
            if (argv[0][2] && (i = str2race(&argv[0][2])) >= 0)
                flags.initrace = i;
            break;
        case '@':
            flags.randomall = 1;
            break;
        default:
            break;
        }
    }
    locknum = sysopt.maxplayers;
}

/* Does the game that wrote this pid into its level 0 still run? */
static boolean
pid_runs(int pid)
{
    HANDLE h = OpenProcess(SYNCHRONIZE, FALSE, (DWORD) pid);
    boolean runs = FALSE;

    if (h) {
        runs = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
    } else if (GetLastError() == ERROR_ACCESS_DENIED) {
        runs = TRUE; /* it exists, under another user */
    }
    return runs;
}

/* unixunix.c's: a lock that is old (three days) or whose game is gone */
static boolean
veryold(int fd)
{
    struct stat buf;
    int lockedpid;

    if (fstat(fd, &buf))
        return FALSE; /* cannot get status */
    if (time((time_t *) 0) - buf.st_mtime < 3L * 24L * 60L * 60L) {
        if (read(fd, (genericptr_t) &lockedpid, sizeof lockedpid)
            != sizeof lockedpid)
            return FALSE;
        if (pid_runs(lockedpid))
            return FALSE;
    }
    return TRUE;
}

static boolean
eraseoldlocks(void)
{
    int i;

    program_state.preserve_locks = 0;
    for (i = 1; i <= MAXDUNGEON * MAXLEVEL + 1; i++) {
        set_levelfile_name(gl.lock, i);
        (void) unlink(fqname(gl.lock, LEVELPREFIX, 0));
    }
    set_levelfile_name(gl.lock, 0);
    return !unlink(fqname(gl.lock, LEVELPREFIX, 0));
}

/* unixunix.c's getlock(): the first free slot of "alock".."zlock" (by the
   character's name in wizard mode), then an empty level 0 with our pid */
static void
rh_getlock(void)
{
    static const char destroy_old_game_prompt[] =
    "There is already a game in progress under your name.  Destroy old game?";
    int i = 0, fd, too_old;
    const char *fq_lock;

    if (!lock_file(HLOCK, LOCKPREFIX, 10)) {
        wait_synch();
        error("%s", "");
    }
    if (!locknum)
        Sprintf(gl.lock, "%u%s", (unsigned) getuid(), svp.plname);
    else
        Strcpy(gl.lock, "1lock"); /* the '1' becomes 'a', 'b', &c below */
    regularize(gl.lock);
    set_levelfile_name(gl.lock, 0);

    if (locknum) {
        if (locknum > 25)
            locknum = 25;
        do {
            gl.lock[0] = 'a' + i++;
            fq_lock = fqname(gl.lock, LEVELPREFIX, 0);
            if ((fd = open(fq_lock, O_RDONLY | O_BINARY)) == -1) {
                if (errno == ENOENT)
                    goto gotlock;
                unlock_file(HLOCK);
                error("Cannot open %s", fq_lock);
            }
            too_old = veryold(fd);
            (void) close(fd);
            if (too_old && eraseoldlocks())
                goto gotlock;
        } while (i < locknum);
        unlock_file(HLOCK);
        error("Too many hacks running now.");
    } else {
        fq_lock = fqname(gl.lock, LEVELPREFIX, 0);
        if ((fd = open(fq_lock, O_RDONLY | O_BINARY)) == -1) {
            if (errno == ENOENT)
                goto gotlock;
            unlock_file(HLOCK);
            error("Cannot open %s", fq_lock);
        }
        too_old = veryold(fd);
        (void) close(fd);
        if (too_old && eraseoldlocks())
            goto gotlock;
        unlock_file(HLOCK);
        if (iflags.window_inited && y_n(destroy_old_game_prompt) == 'y') {
            if (!eraseoldlocks())
                error("Couldn't destroy old game.");
        } else {
            error("%s", "");
        }
    }

 gotlock:
    fd = open(fq_lock, O_WRONLY | O_BINARY | O_CREAT | O_TRUNC, FCMASK);
    if (fd == -1)
        error("cannot creat lock file (%s).", fq_lock);
    if (write(fd, (genericptr_t) &svh.hackpid, sizeof svh.hackpid)
        != sizeof svh.hackpid)
        error("cannot write lock (%s)", fq_lock);
    if (close(fd) == -1)
        error("cannot close lock (%s)", fq_lock);
    unlock_file(HLOCK);
}

static void
wd_message(void)
{
    if (iflags.wiz_error_flag) {
        You("cannot access debug (wizard) mode.");
        wizard = FALSE;
        if (!iflags.explore_error_flag)
            pline("Entering explore/discovery mode instead.");
    } else if (iflags.explore_error_flag) {
        You("cannot access explore mode.");
        discover = iflags.deferred_X = FALSE;
    } else if (discover) {
        You("are in non-scoring explore/discovery mode.");
    }
}

/* sysconf's WIZARDS and EXPLORERS: "*" or a list holding the character's
   name (there is no login name to check on Windows) */
static boolean
check_user_string(const char *optstr)
{
    size_t len = strlen(svp.plname);
    const char *w = optstr;

    if (optstr[0] == '*')
        return TRUE;
    while (*w) {
        while (*w && isspace((uchar) *w))
            w++;
        if (len && !strncmp(w, svp.plname, len)
            && (!w[len] || isspace((uchar) w[len])))
            return TRUE;
        while (*w && !isspace((uchar) *w))
            w++;
    }
    return FALSE;
}

boolean
authorize_wizard_mode(void)
{
    if (sysopt.wizards && sysopt.wizards[0]
        && check_user_string(sysopt.wizards))
        return TRUE;
    iflags.wiz_error_flag = TRUE;
    return FALSE;
}

boolean
authorize_explore_mode(void)
{
    if (sysopt.explorers && sysopt.explorers[0]
        && check_user_string(sysopt.explorers))
        return TRUE;
    iflags.explore_error_flag = TRUE;
    return FALSE;
}

/* the help menu's port-specific entry */
void
port_help(void)
{
    display_file(PORT_HELP, TRUE);
}

/* RENETHACK_SEED, as libnhmain.c's sys_random_seed() honours it on unix
   (patch 0001): windsys.c's seed is wrapped at link time
   (-Wl,--wrap=sys_random_seed), so the same seed plays the same game */
unsigned long __real_sys_random_seed(void);
unsigned long __wrap_sys_random_seed(void);

unsigned long
__wrap_sys_random_seed(void)
{
    const char *fixed = getenv("RENETHACK_SEED");

    if (fixed && *fixed) {
        /* deterministic run for tests/replays: never reseed */
        has_strong_rngseed = FALSE;
        return strtoul(fixed, (char **) 0, 10);
    }
    return __real_sys_random_seed();
}
