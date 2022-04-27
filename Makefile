## This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

#
# Copyright 2019 Joyent, Inc
# Copyright 2022 MNX Cloud, Inc
#

#
# Variables
#

NAME = rust-cueball
CARGO ?= cargo
# See TOOLS-2546 for why this is disabled
# RUST_CLIPPY_ARGS ?= -- -D clippy::all
CARGO_CHECK_ARGS ?= --workspace

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
	@# Use `cargo check <args>` instead of `cargo clippy <args>`
	@# (TOOLS-2546, as above)
	$(CARGO) clean && $(CARGO) check $(CARGO_CHECK_ARGS)
