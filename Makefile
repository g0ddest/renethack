# renethack: top-level entry points
#   make              build the engine (engine/build/nh-engine, recover, data)
#   make client       build the Godot extension and import the Godot project
#   make run          play: engine + client, then start Godot ($(GODOT))
#   make test         engine tests, then every Rust test against the fresh engine
#   make test-client  headless self-tests of the Godot client, one process each
#   make lint         rustfmt and clippy, warnings are errors

GODOT ?= godot
GODOT_PROJECT := client/godot
SELFTESTS := smoke keys save close crash menus text

.PHONY: all engine client run test test-client lint
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

# every scenario in its own Godot process and playground; a scenario passes
# only with exit status 0 and its "SELFTEST PASS" line
test-client: all client
	@set -e; for s in $(SELFTESTS); do \
		pg=$$(mktemp -d); log=$$pg/selftest.log; \
		echo "selftest $$s"; \
		status=0; timeout 180 $(GODOT) --headless --path $(GODOT_PROJECT) \
			-- --selftest=$$s --playground=$$pg/playground > $$log 2>&1 || status=$$?; \
		if [ $$status -ne 0 ] || ! grep -q 'Initialize godot-rust' $$log \
			|| ! grep -q "SELFTEST PASS $$s" $$log; then \
			tail -40 $$log; echo "selftest $$s FAILED (status $$status)" >&2; exit 1; \
		fi; \
		rm -rf $$pg; \
	done; echo "selftests passed: $(SELFTESTS)"

lint:
	cd client/rust && cargo fmt --all -- --check
	cd client/rust && cargo clippy --all-targets -- -D warnings
