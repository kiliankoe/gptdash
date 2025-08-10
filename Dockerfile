# Multi-stage Dockerfile for GPTdash
# Stage 1: Build frontend
FROM node:24-alpine AS frontend
WORKDIR /app/frontend

# Copy frontend package files
COPY frontend/package*.json ./
RUN npm ci

# Copy frontend source and build
COPY frontend/ ./
RUN npm run build

# Stage 2: Build Go backend with embedded frontend
FROM golang:1.24-alpine AS backend
RUN apk add --no-cache git ca-certificates tzdata

WORKDIR /app

# Copy go mod files
COPY backend/go.mod backend/go.sum ./backend/
WORKDIR /app/backend
RUN go mod download

# Copy backend source
WORKDIR /app
COPY backend/ ./backend/

# Copy built frontend from previous stage
COPY --from=frontend /app/frontend/dist ./backend/static/dist

# Build the binary
WORKDIR /app/backend
RUN CGO_ENABLED=0 GOOS=linux go build -a -installsuffix cgo -o ../gptdash ./cmd/server

# Stage 3: Final runtime image
FROM alpine:latest
RUN apk --no-cache add ca-certificates tzdata

WORKDIR /app
COPY --from=backend /app/gptdash .

# Create non-root user for security
RUN addgroup -g 1000 gptdash && \
    adduser -u 1000 -G gptdash -s /bin/sh -D gptdash

USER gptdash

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/api/health || exit 1

# Run the binary
CMD ["./gptdash"]
