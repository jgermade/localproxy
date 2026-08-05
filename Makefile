.PHONY: help fmt fmt-check clippy check test check-lint ci

help:
	@echo "Targets disponibles:"
	@echo "  make fmt         # Aplica rustfmt"
	@echo "  make fmt-check   # Verifica formato (CI)"
	@echo "  make clippy      # Lint estricto (CI)"
	@echo "  make check       # Compilacion rapida"
	@echo "  make test        # Ejecuta tests"
	@echo "  make check-lint  # Replica Check & Lint del CI"
	@echo "  make ci          # Alias de check-lint"

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

check:
	cargo check --all-targets

test:
	cargo test --all-targets

check-lint: fmt-check clippy check test

ci: check-lint
