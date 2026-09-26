# renethack: top-level entry points
#   make          build the engine (engine/build/nh-engine, engine/build/data)
#   make test     engine tests, then every Rust test against the fresh engine
#   make lint     rustfmt and clippy, warnings are errors

.PHONY: all engine test lint
all: engine

engine:
	$(MAKE) -C engine

test: engine
	$(MAKE) -C engine test
	cd client/rust && cargo test

lint:
	cd client/rust && cargo fmt --all -- --check
	cd client/rust && cargo clippy --all-targets -- -D warnings
