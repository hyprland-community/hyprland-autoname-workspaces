BIN := hyprland-autoname-workspaces
VERSION := 0.3.7

PREFIX ?= /usr
LIB_DIR = $(DESTDIR)$(PREFIX)/lib
BIN_DIR = $(DESTDIR)$(PREFIX)/bin
SHARE_DIR = $(DESTDIR)$(PREFIX)/share

.PHONY: build-dev
build-dev:
	cargo update
	cargo build

.PHONY: build
build:
	cargo update
	cargo build --locked --release

.PHONY: test
test:
	cargo test --locked

.PHONY: lint
lint:
	cargo fmt -- --check
	cargo clippy -- -Dwarnings

.PHONY: run
run:
	cargo run

.PHONY: clean
clean:
	rm -rf dist

.PHONY: install
install:
	install -Dm755 -t "$(BIN_DIR)/" "target/release/$(BIN)"
	install -Dm644 -t "$(LIB_DIR)/systemd/user" "$(BIN).service"
	install -Dm644 -t "$(SHARE_DIR)/licenses/$(BIN)/" LICENSE.md

.PHONY: dist
dist: clean build
	mkdir -p dist
	cp "target/release/$(BIN)" .
	tar -czvf "dist/$(BIN)-$(VERSION)-linux-x86_64.tar.gz" "$(BIN)" "$(BIN).service" LICENSE.md README.md Makefile
	git archive -o "dist/$(BIN)-$(VERSION).tar.gz" --format tar.gz --prefix "$(BIN)-$(VERSION)/" "$(VERSION)"
	for f in dist/*.tar.gz; do gpg --detach-sign --armor "$$f"; done
	rm -f "dist/$(BIN)-$(VERSION).tar.gz" "$(BIN)"
