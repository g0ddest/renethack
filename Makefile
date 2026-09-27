# renethack: top-level entry points
#   make              build the engine (engine/build/nh-engine, recover, data)
#   make client       build the Godot extension and import the Godot project
#   make run          play: engine + client, then start Godot ($(GODOT))
#   make test         engine tests, then every Rust test against the fresh engine
#   make test-client  headless self-tests of the Godot client, one process each
#   make soak         random play through the client UI, seeds 1..8
#   make lint         rustfmt and clippy, warnings are errors

GODOT ?= godot
GODOT_PROJECT := client/godot
SELFTESTS := smoke keys save close crash menus text soak
# answered requests of the soak in test-client (about 35 s; from 1000 on the
# soak fails unless the level changes); `make soak` runs the default, 2000
SOAK_CI := 2000
SOAK_SEEDS := 1 2 3 4 5 6 7 8

.PHONY: all engine client run test test-client soak lint
all: engine

engine:
	$(MAKE) -C engine

# A fresh checkout has no client/godot/.godot: until the project is imported
# Godot does not load the extension and the main scene stays an empty
# placeholder.  The first import of a project with a GDExtension may crash on
# exit after writing extension_list.cfg, so its status is ignored and the
# file is checked instead.
client:
	cd client/rust && cargo build -p renethack-gd
	@if [ ! -f $(GODOT_PROJECT)/.godot/extension_list.cfg ]; then \
		$(GODOT) --headless --path $(GODOT_PROJECT) --import > /dev/null 2>&1; \
		test -f $(GODOT_PROJECT)/.godot/extension_list.cfg \
			|| { echo "Godot import of $(GODOT_PROJECT) failed" >&2; exit 1; }; \
	fi

run: all client
	$(GODOT) --path $(GODOT_PROJECT)

test: engine
	$(MAKE) -C engine test
	cd client/rust && cargo test

# One self-test in its own Godot process and playground: $(1) scenario,
# $(2) more arguments, $(3) timeout in seconds, $(4) its name in messages.
# It passes only with exit status 0 and its "SELFTEST PASS" line.
define run_selftest
pg=$$(mktemp -d); log=$$pg/selftest.log; \
echo "selftest $(4)"; \
status=0; timeout $(3) $(GODOT) --headless --path $(GODOT_PROJECT) \
	-- --selftest=$(1) $(2) --playground=$$pg/playground > $$log 2>&1 || status=$$?; \
if [ $$status -ne 0 ] || ! grep -q 'Initialize godot-rust' $$log \
	|| ! grep -q "SELFTEST PASS $(1)" $$log; then \
	tail -60 $$log; echo "selftest $(4) FAILED (status $$status)" >&2; exit 1; \
fi; \
grep '^selftest: soak: [0-9]* requests' $$log || true; \
rm -rf $$pg
endef

# every scenario; the soak with seed 42 and $(SOAK_CI) requests
test-client: all client
	@set -e; for s in $(SELFTESTS); do \
		args=""; if [ $$s = soak ]; then args="--soak=$(SOAK_CI)"; fi; \
		$(call run_selftest,$$s,$$args,180,$$s $$args); \
	done; echo "selftests passed: $(SELFTESTS)"

# random play through the UI with each of $(SOAK_SEEDS) and the default budget
soak: all client
	@set -e; for seed in $(SOAK_SEEDS); do \
		$(call run_selftest,soak,--seed=$$seed,900,soak seed $$seed); \
	done; echo "soak passed with seeds $(SOAK_SEEDS)"

lint:
	cd client/rust && cargo fmt --all -- --check
	cd client/rust && cargo clippy --all-targets -- -D warnings
