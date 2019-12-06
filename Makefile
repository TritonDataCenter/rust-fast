#
# Copyright 2019 Joyent, Inc
#

#
# Variables
#

NAME = rust-cueball
CARGO ?= cargo
RUST_CLIPPY_ARGS ?= -- -D clippy::all

#
# Repo-specific targets
#
.PHONY: all
all: build-cueball

.PHONY: build-cueball
build-cueball:
	$(CARGO) build --release

.PHONY: test
test:
	$(CARGO) test

.PHONY: check
check:
	$(CARGO) clean && $(CARGO) clippy $(RUST_CLIPPY_ARGS)
