# Makefile for building and installing the loco binary

# Define variables
BINARY_NAME=loco
TARGET_DIR=target/release
INSTALL_DIR=/usr/local/bin

.PHONY: all build install clean

# Default target
all: build install

# Build the project in release mode
build:
	@echo "🚀 Compiling the $$(BINARY_NAME)..."
	cargo build --release

# Install the binary to system PATH
install: build
	@echo "🔧 Installing $$(BINARY_NAME) to the system..."
	sudo cp $(TARGET_DIR)/$(BINARY_NAME) $(INSTALL_DIR)/$(BINARY_NAME)
	@echo "✅ $$(BINARY_NAME) is now available globally! You can run it from anywhere."

# Clean the build artifacts
clean:
	@echo "🧹 Cleaning up build artifacts..."
	cargo clean
	@echo "🎉 Cleaned up successfully!"
