.PHONY: build install run install-pi check install-timer uninstall-timer clean

build:
	cargo build --release

run:
	ABLAUFDATUM_PASSWORD=$${ABLAUFDATUM_PASSWORD:-test} cargo run --bin ablaufdatum-tracker

install: build
	@cp target/release/ablaufdatum-tracker .
	@cp target/release/ablaufdatum-checker .
	./install.sh

install-pi: build
	sudo cp target/release/ablaufdatum-tracker /usr/local/bin/
	sudo cp target/release/ablaufdatum-checker /usr/local/bin/
	sudo chmod 755 /usr/local/bin/ablaufdatum-tracker /usr/local/bin/ablaufdatum-checker
	sudo systemctl restart ablaufdatum-web.service
	@echo "==> Binaries installiert und Dienst neu gestartet."

check:
	ablaufdatum-checker

install-timer:
	systemctl --user enable --now ablaufdatum-checker.timer

uninstall-timer:
	systemctl --user disable --now ablaufdatum-checker.timer

clean:
	cargo clean
	rm -f ablaufdatum-tracker ablaufdatum-checker
