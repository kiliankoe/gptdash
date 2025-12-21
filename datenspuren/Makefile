FRONTEND_DIR=frontend
BACKEND_DIR=backend
BIN=gptdash

# Get version from git tag, fallback to commit hash if no tags
VERSION := $(shell git describe --tags --exact-match 2>/dev/null || git describe --always --dirty)

.PHONY: all build frontend backend clean version

all: build

frontend:
	echo "export const VERSION = \"$(VERSION)\";" > $(FRONTEND_DIR)/src/version.ts
	cd $(FRONTEND_DIR) && npm install && npm run build

backend:
	# copy built frontend into backend/static/dist for embedding
	rm -rf $(BACKEND_DIR)/static/dist
	mkdir -p $(BACKEND_DIR)/static/dist
	cp -R $(FRONTEND_DIR)/dist/* $(BACKEND_DIR)/static/dist/
	cd $(BACKEND_DIR) && go mod tidy && go build -ldflags "-X main.version=$(VERSION)" -o ../$(BIN) ./cmd/server

build: frontend backend

clean:
	rm -f $(BIN)
	rm -rf $(FRONTEND_DIR)/dist
	rm -f $(FRONTEND_DIR)/src/version.ts

version:
	@echo $(VERSION)
