# renethack engine host build fragment.
# Run from the patched NetHack tree's src/ directory:
#   make -f <this file> WANT_LIBNH=1 HACKDIR=. NO_NHUUID=1 \
#        RH_HOST=<engine/host> RH_PATCHSET=<id> nh-engine
# Including the generated Makefile gives us NetHack's exact compiler flags.
include Makefile

RH_PATCHSET ?= unknown
# unixmain.o duplicates libnhmain.o; the tty port is not built for libnh
RH_EXCLUDE = unixmain.o getline.o termcap.o topl.o wintty.o
RH_CORE_OBJS = $(sort $(filter-out $(RH_EXCLUDE),$(HOBJ)) \
		$(LIBNHSYSOBJ) date.o tile.o)
RH_OBJS = rh_main.o rh_bridge.o rh_catalog.o rh_fmt.o rh_proto.o \
		rh_progress.o rh_time.o rh_cjson.o
RH_CFLAGS = $(TARGET_CFLAGS) -I$(RH_HOST) -I$(RH_HOST)/third_party/cjson \
		-DRH_PATCHSET=\"$(RH_PATCHSET)\"

rh_%.o: $(RH_HOST)/rh_%.c $(RH_HOST)/rh_proto.h $(RH_HOST)/rh_bridge.h \
		$(RH_HOST)/rh_fmt.h $(RH_HOST)/rh_progress.h $(HACK_H)
	$(TARGET_CC) $(RH_CFLAGS) -c -o $@ $<

rh_cjson.o: $(RH_HOST)/third_party/cjson/cJSON.c
	$(TARGET_CC) -O2 -w -c -o $@ $<

nh-engine: $(RH_OBJS) $(RH_CORE_OBJS) hacklib.a
	$(TARGET_CC) $(TARGET_LFLAGS) -o $@ $(RH_OBJS) $(RH_CORE_OBJS) \
		hacklib.a $(LUALIBS)
