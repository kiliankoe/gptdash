FRONTEND_DIR=frontend
BACKEND_DIR=backend
BIN=gptdash

.PHONY: all build frontend backend clean

all: build

frontend:
	cd $(FRONTEND_DIR) && npm install && npm run build

backend:
	# copy built frontend into backend/static/dist for embedding
	rm -rf $(BACKEND_DIR)/static/dist
	mkdir -p $(BACKEND_DIR)/static/dist
	cp -R $(FRONTEND_DIR)/dist/* $(BACKEND_DIR)/static/dist/
	cd $(BACKEND_DIR) && go mod tidy && go build -o ../$(BIN) ./cmd/server

build: frontend backend

clean:
	rm -f $(BIN)
	rm -rf $(FRONTEND_DIR)/dist
