
# Makefile for building and installing the loco binary

# Define variables
BINARY_NAME = loco
TARGET_DIR  = target/release
INSTALL_DIR = /usr/local/bin

.PHONY: all build install clean check_cargo

# Default target
all: check_cargo build install

# Check if cargo is installed
check_cargo:
	@echo "🔍 Checking if Cargo is installed..."
	@if ! command -v cargo &> /dev/null; then \
		echo "❌ Cargo not found! Installing..."; \
		# For Ubuntu/Debian
		sudo apt update && sudo apt install -y cargo; \
		echo "✅ Cargo installed!"; \
	else \
		echo "✅ Cargo is already installed."; \
	fi

# Build the project in release mode
build:
	@echo "🚀 Compiling $(BINARY_NAME)..."
	cargo build --release

# Install the binary to system PATH
install: build
	@echo "🔧 Installing $(BINARY_NAME) to the system..."
	sudo cp $(TARGET_DIR)/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "✅ $(BINARY_NAME) is now available globally! You can run it from anywhere."

# Clean the build artifacts
clean:
	@echo "🧹 Cleaning up build artifacts..."
	cargo clean
	@echo "🎉 Cleaned up successfully!"
