# renethack engine for Windows: NetHack's WIN32 configuration with the shim
# window port and the protocol host, cross-compiled with mingw-w64
# (engine/Makefile's `windows` target, in tools/dist/windows's image).
# Run from the patched tree's src/ once the native build has made the data
# files and the generated sources (tile.c, nhlua.h):
#   make -f <this file> RH_HOST=<engine/host> RH_PATCHSET=<id> OUT=<dir> windows
# Including the generated Makefile gives us NetHack's list of core sources.
include Makefile

RH_PATCHSET ?= unknown
WCC = x86_64-w64-mingw32-gcc
WCXX = x86_64-w64-mingw32-g++
WSTRIP = x86_64-w64-mingw32-strip
O = $(OUT)/obj
# NetHack's Windows files #include <ShlObj.h> and <sys\stat.h>; mingw's
# headers are lower case, and here '\' is no separator
SHIMS = $(OUT)/include
LUASRC = $(firstword $(wildcard ../lib/lua-*/src))
# sys/windows/GNUmakefile's definitions with the shim window port rather than
# the console; rh_wintime.h sends every time() to the host's test clock;
# mingw's printf for C99's %zu
WTIME = -include $(RH_HOST)/win32/rh_wintime.h
WCFLAGS = -O2 -mms-bitfields -I../include -I../sys/windows -I$(SHIMS) \
	-I$(LUASRC) -I$(RH_HOST) -I$(RH_HOST)/third_party/cjson $(WTIME) \
	-DWIN32 -DWINVER=0x0601 -D_WIN32_WINNT=0x0601 -DDLB \
	-DSHIM_GRAPHICS -DNOTTYGRAPHICS -DLIBNH -DNOSHELL \
	-D__USE_MINGW_ANSI_STDIO=1 -DRH_PATCHSET=\"$(RH_PATCHSET)\"
WLIBS = -static -Wl,--wrap=sys_random_seed -luserenv -lole32 -lwinmm -luuid -lcomctl32 -lgdi32 \
	-lshell32 -ladvapi32 -lbcrypt -lrpcrt4

# HACKCSRC lacks cfgfiles.c, which the unix Makefile links all the same
W_CORE = $(HACKCSRC) cfgfiles.c date.c tile.c windsys.c random.c winshim.c
W_HOST = rh_main.c rh_bridge.c rh_catalog.c rh_fmt.c rh_held.c rh_proto.c \
	rh_progress.c rh_time.c rh_winmain.c rh_wincompat.c
W_LUA = $(filter-out lua.c luac.c,$(notdir $(wildcard $(LUASRC)/*.c)))
W_OBJS = $(addprefix $(O)/,$(W_CORE:.c=.o) $(W_HOST:.c=.o) cppregex.o \
	rh_cjson.o) $(addprefix $(O)/lua/,$(W_LUA:.c=.o))

vpath %.c ../sys/windows ../sys/share ../win/shim $(RH_HOST) $(RH_HOST)/win32

.PHONY: windows
windows: $(OUT)/nh-engine.exe $(OUT)/recover.exe

$(OUT)/nh-engine.exe: $(W_OBJS)
	$(WCXX) -o $@ $(W_OBJS) $(WLIBS)
	$(WSTRIP) $@

# NetHack's recover, as sys/windows/GNUmakefile builds it
$(OUT)/recover.exe: $(O)/recover.o $(O)/rversion.o $(O)/hacklib.o
	$(WCC) -o $@ $^ -static
	$(WSTRIP) $@

$(SHIMS)/.made:
	mkdir -p $(SHIMS)
	for h in ShlObj VersionHelpers UserEnv BaseTsd; do \
		echo "#include <`echo $$h | tr A-Z a-z`.h>" > $(SHIMS)/$$h.h; \
	done
	echo '#include <sys/stat.h>' > '$(SHIMS)/sys\stat.h'
	echo '#include <sys/types.h>' > '$(SHIMS)/sys\types.h'
	touch $@

# any header may change any object: rebuilt together
W_HEADERS = $(wildcard ../include/*.h ../sys/windows/*.h $(RH_HOST)/*.h $(RH_HOST)/win32/*.h)

$(O)/%.o: %.c $(SHIMS)/.made $(W_HEADERS)
	@mkdir -p $(@D)
	$(WCC) $(WCFLAGS) -c -o $@ $<

$(O)/cppregex.o: ../sys/share/cppregex.cpp $(SHIMS)/.made
	@mkdir -p $(@D)
	$(WCXX) -O2 -I../include -I../sys/windows -I$(SHIMS) -DWIN32 -DDLB \
		-DSHIM_GRAPHICS -DNOTTYGRAPHICS -c -o $@ $<

$(O)/rh_cjson.o: $(RH_HOST)/third_party/cjson/cJSON.c
	@mkdir -p $(@D)
	$(WCC) -O2 -w -c -o $@ $<

$(O)/recover.o: ../util/recover.c $(SHIMS)/.made
	@mkdir -p $(@D)
	$(WCC) $(WCFLAGS) -c -o $@ $<

$(O)/rversion.o: version.c $(SHIMS)/.made
	@mkdir -p $(@D)
	$(WCC) $(WCFLAGS) -DMINIMAL_FOR_RECOVER -c -o $@ $<

$(O)/lua/%.o: $(LUASRC)/%.c
	@mkdir -p $(@D)
	$(WCC) -O2 -w -DLUA_COMPAT_5_3 $(WTIME) -c -o $@ $<
