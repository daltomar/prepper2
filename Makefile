.PHONY: build install run check install-timer uninstall-timer clean

build:
	cargo build --release

run:
	cargo run --bin ablaufdatum-tracker

install: build
	@cp target/release/ablaufdatum-tracker .
	@cp target/release/ablaufdatum-checker .
	./install.sh

check:
	ablaufdatum-checker

install-timer:
	systemctl --user enable --now ablaufdatum-checker.timer

uninstall-timer:
	systemctl --user disable --now ablaufdatum-checker.timer

clean:
	cargo clean
	rm -f ablaufdatum-tracker ablaufdatum-checker
