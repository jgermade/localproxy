.PHONY: help fmt fmt-check clippy check check-i686 test check-lint ci

REQUIRE_I686 ?= 0
I686_TARGET := i686-unknown-linux-gnu
I686_STDLIB := $(shell rustc --print sysroot 2>/dev/null)/lib/rustlib/$(I686_TARGET)/lib

help:
	@echo "Targets disponibles:"
	@echo "  make fmt         # Aplica rustfmt"
	@echo "  make fmt-check   # Verifica formato (CI)"
	@echo "  make clippy      # Lint estricto (CI)"
	@echo "  make check       # Compilacion rapida"
	@echo "  make check-i686  # Compilacion tipo-check para Linux i686 (omite si no esta instalado)"
	@echo "  make test        # Ejecuta tests"
	@echo "  make check-lint  # Checks locales (omite i686 si falta el target)"
	@echo "  make ci          # Alias de check-lint"
	@echo "  make ci REQUIRE_I686=1  # Replica CI incluyendo i686"

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

check:
	cargo check --all-targets

check-i686:
	@if command -v rustup >/dev/null 2>&1; then rustup target add i686-unknown-linux-gnu; fi
	@if [ -d "$(I686_STDLIB)" ]; then \
		cargo check --all-targets --target $(I686_TARGET); \
	elif [ "$(REQUIRE_I686)" = "1" ]; then \
		echo "Fallo check-i686: falta el target $(I686_TARGET) en esta toolchain."; \
		echo "Si usas rustup: rustup target add $(I686_TARGET)"; \
		exit 1; \
	else \
		echo "Omitiendo check-i686: falta el target $(I686_TARGET) en esta toolchain local."; \
		echo "Para exigirlo: make ci REQUIRE_I686=1"; \
	fi

test:
	cargo test --all-targets

check-lint: fmt-check clippy check check-i686 test

ci: check-lint
