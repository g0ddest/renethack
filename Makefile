# renethack: top-level entry points
#   make              build the engine (engine/build/nh-engine, recover, data)
#   make client       build the Godot extension; import the Godot project and
#                     any new or changed art
#   make run          play: engine + client, then start Godot ($(GODOT))
#   make test         engine tests, then every Rust test against the fresh engine
#   make test-client  headless self-tests of the Godot client, one process each
#   make deck         screenshots at the Steam Deck's 1280×800 (needs a display)
#   make soak         random play through the client UI, seeds 1..8
#   make art          fetch the CC0 art again (tools/fetch_art.py; needs Pillow)
#   make lint         rustfmt and clippy, warnings are errors

GODOT ?= godot
# a macOS app bundle names a directory; the binary is inside it
override GODOT := $(if $(filter %.app %.app/,$(GODOT)),$(patsubst %/,%,$(GODOT))/Contents/MacOS/Godot,$(GODOT))
# GNU coreutils' timeout; Homebrew's coreutils names it gtimeout on macOS
TIMEOUT ?= $(shell command -v timeout || command -v gtimeout)
GODOT_PROJECT := client/godot
SELFTESTS := smoke keys save close crash menus text dialogs moves orders inventory bar gamepad equipment item-use combat roles branches soak
# again at the Steam Deck's 1280×800 (its 120 % UI scale, the compact
# layout): every screen a scenario would shoot must fit the canvas
DECK_SELFTESTS := smoke inventory hud gamepad
# `make deck`: the same screens shot at a real 1280×800, into DECK_DIR
DECK_SHOTS := smoke tour inventory bar hud dialogs gamepad
DECK_DIR ?= $(GODOT_PROJECT)/.godot/shots/deck
# answered requests of the soak in test-client (about 35 s; from 1000 on the
# soak fails unless the level changes); `make soak` runs the default, 2000
SOAK_CI := 2000
SOAK_SEEDS := 1 2 3 4 5 6 7 8

.PHONY: all engine client import run test test-client soak lint need-timeout art icons achievement-icons deck
all: engine

engine:
	$(MAKE) -C engine

# Godot must import the art (and, on a fresh checkout, the extension) before
# a run outside the editor can load it: without client/godot/.godot the main
# scene stays an empty placeholder, and art added or changed since the last
# import does not load.  The import is incremental and runs whenever a file
# under client/godot/art or the project's own files is newer than its stamp.
# The first import of a project with a GDExtension may crash on exit after
# writing extension_list.cfg, so its status is ignored and the file is
# checked instead.
IMPORT_STAMP := $(GODOT_PROJECT)/.godot/renethack-import.stamp
IMPORT_INPUTS := $(shell find $(GODOT_PROJECT)/art $(GODOT_PROJECT)/fonts $(GODOT_PROJECT)/ui -type f 2>/dev/null) \
	$(GODOT_PROJECT)/project.godot $(GODOT_PROJECT)/renethack.gdextension \
	$(GODOT_PROJECT)/main.tscn

client:
	cd client/rust && cargo build -p renethack-gd
	@$(MAKE) --no-print-directory import

import: $(IMPORT_STAMP)

$(IMPORT_STAMP): $(IMPORT_INPUTS)
	@command -v "$(GODOT)" > /dev/null || [ -x "$(GODOT)" ] || { echo "Godot not found: GODOT=$(GODOT)" \
		"(pass the executable, e.g. GODOT=/Applications/Godot.app)" >&2; exit 1; }
	@echo "importing the Godot project (art, extension)"
	@mkdir -p $(GODOT_PROJECT)/.godot
	@$(GODOT) --headless --path $(GODOT_PROJECT) --import > $(GODOT_PROJECT)/.godot/import.log 2>&1 || true
	@test -f $(GODOT_PROJECT)/.godot/extension_list.cfg \
		|| { tail -30 $(GODOT_PROJECT)/.godot/import.log >&2; \
		     echo "Godot import of $(GODOT_PROJECT) failed (log: $(GODOT_PROJECT)/.godot/import.log)" >&2; exit 1; }
	@touch $@

run: all client
	$(GODOT) --path $(GODOT_PROJECT)

test: engine
	$(MAKE) -C engine test
	cd client/rust && cargo test

# One self-test in its own Godot process and playground: $(1) scenario,
# $(2) more arguments, $(3) timeout in seconds, $(4) its name in messages,
# $(5) empty for a headless run, else it gets a window (screenshots).
# It passes only with exit status 0 and its "SELFTEST PASS" line.
define run_selftest
pg=$$(mktemp -d); log=$$pg/selftest.log; \
echo "selftest $(4)"; \
status=0; $(TIMEOUT) $(3) $(GODOT) $(if $(5),,--headless) --path $(GODOT_PROJECT) \
	-- --selftest=$(1) $(2) --playground=$$pg/playground > $$log 2>&1 || status=$$?; \
if [ $$status -ne 0 ] || ! grep -q 'Initialize godot-rust' $$log \
	|| ! grep -q "SELFTEST PASS $(1)" $$log; then \
	tail -60 $$log; echo "selftest $(4) FAILED (status $$status)" >&2; exit 1; \
fi; \
grep '^selftest: soak: [0-9]* requests' $$log || true; \
rm -rf $$pg
endef

need-timeout:
	@if [ -z "$(TIMEOUT)" ]; then \
		echo "the self-tests need GNU timeout: install coreutils" \
			"(macOS: brew install coreutils), or pass TIMEOUT=..." >&2; \
		exit 1; \
	fi

# every scenario but tour (map screenshots); the soak with seed 42 and
# $(SOAK_CI) requests; then $(DECK_SELFTESTS) at the Deck's size
test-client: need-timeout all client
	@set -e; for s in $(SELFTESTS); do \
		args=""; if [ $$s = soak ]; then args="--soak=$(SOAK_CI)"; fi; \
		$(call run_selftest,$$s,$$args,180,$$s $$args); \
	done; \
	for s in $(DECK_SELFTESTS); do \
		$(call run_selftest,$$s,--size=1280x800,180,$$s at 1280x800); \
	done; echo "selftests passed: $(SELFTESTS); at 1280x800: $(DECK_SELFTESTS)"

# The Steam Deck's screen for review: $(DECK_SHOTS) shot at a real
# 1280×800 into $(DECK_DIR)/<scenario>. A run fails when the window is not
# that size or a screen does not fit it. Needs a display (without one:
# xvfb-run make deck).
deck: need-timeout all client
	@set -e; for s in $(DECK_SHOTS); do \
		dir=$(abspath $(DECK_DIR))/$$s; rm -rf $$dir; mkdir -p $$dir; \
		$(call run_selftest,$$s,--size=1280x800 --screenshots=$$dir,300,$$s at 1280x800,window); \
	done; echo "Deck screenshots in $(abspath $(DECK_DIR)): $(DECK_SHOTS)"

# random play through the UI with each of $(SOAK_SEEDS) and the default budget
soak: need-timeout all client
	@set -e; for seed in $(SOAK_SEEDS); do \
		$(call run_selftest,soak,--seed=$$seed,900,soak seed $$seed); \
	done; echo "soak passed with seeds $(SOAK_SEEDS)"

# Bake the item icons (client/godot/art/icons/items/<tile>.png) from the
# object art; needs a display, so not headless
icons: all client
	@pg=$$(mktemp -d); $(if $(TIMEOUT),$(TIMEOUT) 600) $(GODOT) --path $(GODOT_PROJECT) \
		-- --selftest=icons --playground=$$pg/playground > $$pg/icons.log 2>&1; \
	grep '^selftest: icons' $$pg/icons.log; \
	grep -q 'SELFTEST PASS icons' $$pg/icons.log || { tail -40 $$pg/icons.log; exit 1; }; \
	rm -rf $$pg

# Bake the achievements' medallions (client/godot/art/icons/achievements/,
# steam/achievements/ and steam/achievements.vdf) from
# client/achievements/achievements.toml; needs a display, so not headless
achievement-icons: all client
	@pg=$$(mktemp -d); $(if $(TIMEOUT),$(TIMEOUT) 900) $(GODOT) --path $(GODOT_PROJECT) \
		-- --selftest=achievement-icons --playground=$$pg/playground > $$pg/achievement-icons.log 2>&1; \
	grep '^selftest: achievement-icons' $$pg/achievement-icons.log; \
	grep -q 'SELFTEST PASS achievement-icons' $$pg/achievement-icons.log \
		|| { tail -40 $$pg/achievement-icons.log; exit 1; }; \
	rm -rf $$pg

# The art is committed; this re-fetches it from Poly Haven and itch.io and
# checks the downloads against client/godot/art/art.lock.json; `make client`
# then imports what changed
art:
	python3 tools/fetch_art.py

lint:
	cd client/rust && cargo fmt --all -- --check
	cd client/rust && cargo clippy --all-targets -- -D warnings
