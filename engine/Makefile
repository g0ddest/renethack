# renethack engine: pristine NetHack (upstream submodule) + patches + libnh
# + our host  ->  build/nh-engine and build/data/.
#
#   make            build everything
#   make test       host unit tests + engine smoke test
#   make clean      remove build/
#
# The upstream submodule is never modified: it is exported into build/tree.

ENGINE_DIR := $(CURDIR)
UPSTREAM ?= $(ENGINE_DIR)/upstream
# relative on purpose: `make -C engine build/...` must name these targets
BUILD := build
TREE := $(BUILD)/tree
HOST := $(ENGINE_DIR)/host

ifeq ($(shell uname -s),Darwin)
HINTS := macOS.500
else
HINTS := linux.500
endif

# WANT_LIBNH: build the core with the shim window port
# HACKDIR=.:  data and sysconf are found relative to the working directory
# NO_NHUUID:  no crash-report UUID support (its code lives in unixmain.o)
NHVARS := WANT_LIBNH=1 HACKDIR=. NO_NHUUID=1
PATCHES := $(shell cat patches/series)
PATCHSET := $(shell cat $(addprefix patches/,$(PATCHES)) | shasum -a 256 | cut -c1-16)
HOST_SRC := $(wildcard $(HOST)/*.c $(HOST)/*.h $(HOST)/rh.mk)

.PHONY: all test clean
all: $(BUILD)/nh-engine $(BUILD)/data/.stamp

$(TREE)/.patched: patches/series $(addprefix patches/,$(PATCHES))
	rm -rf $(TREE)
	mkdir -p $(TREE)
	git -C $(UPSTREAM) archive --format=tar HEAD | tar -x -C $(TREE)
	for p in $(PATCHES); do \
		patch -d $(TREE) -p1 --forward --quiet < patches/$$p || exit 1; \
	done
	cd $(TREE)/sys/unix && sh setup.sh hints/$(HINTS) > /dev/null
	touch $@

$(TREE)/.built: $(TREE)/.patched
	$(MAKE) -C $(TREE) fetch-lua
	$(MAKE) -C $(TREE) $(NHVARS) lua_support
	$(MAKE) -C $(TREE) $(NHVARS) all
	touch $@

$(BUILD)/nh-engine: $(TREE)/.built $(HOST_SRC)
	$(MAKE) -C $(TREE)/src -f $(HOST)/rh.mk $(NHVARS) \
		RH_HOST=$(HOST) RH_PATCHSET=$(PATCHSET) nh-engine
	cp $(TREE)/src/nh-engine $@

$(BUILD)/data/.stamp: $(TREE)/.built runtime/sysconf
	rm -rf $(BUILD)/data
	mkdir -p $(BUILD)/data
	cp $(TREE)/dat/nhdat $(TREE)/dat/license $(TREE)/dat/symbols $(BUILD)/data/
	cp runtime/sysconf $(BUILD)/data/sysconf
	touch $@

$(BUILD)/test_proto: $(HOST)/tests/test_proto.c $(HOST)/rh_proto.c \
		$(HOST)/rh_proto.h $(HOST)/third_party/cjson/cJSON.c
	mkdir -p $(BUILD)
	cc -std=c99 -Wall -Wextra -I$(HOST) -I$(HOST)/third_party/cjson \
		-o $@ $(HOST)/tests/test_proto.c $(HOST)/rh_proto.c \
		$(HOST)/third_party/cjson/cJSON.c

test: $(BUILD)/test_proto all
	$(BUILD)/test_proto
	sh tests/smoke.sh $(BUILD)

clean:
	rm -rf $(BUILD)
