# Makefile for persona-script-editor (Tauri + TypeScript/Vite)
# Requires: Node.js, npm, Rust, MSVC or MinGW, make (GNU Make in MSYS2/MinGW)

PROJECT_DIR := /c/Users/romas/Desktop/Projects/persona-script-editor
SRC_TAURI := $(PROJECT_DIR)/src-tauri

.PHONY: default install build dev clean release help

default: build

install:
	cd $(PROJECT_DIR) && npm install
	cd $(SRC_TAURI) && cargo install

build:
	cd $(PROJECT_DIR) && npm run tauri build

dev:
	cd $(PROJECT_DIR) && npm run tauri dev

clean:
	cd $(PROJECT_DIR) && npm clean
	cd $(SRC_TAURI) && cargo clean
	rm -rf $(SRC_TAURI)/target
	rm -f *.log

release: build

help:
	@echo Available commands:
	@echo default  - build the application (npm run tauri build)
	@echo install  - install dependencies (npm install + cargo install)
	@echo build    - build the final application (Windows .exe/.msi)
	@echo dev      - run in development mode with hot reload
	@echo clean    - remove dependencies and target directory
	@echo release  - alias for build
	@echo help     - show this help message