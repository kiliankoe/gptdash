package static

import (
	"embed"
	"io/fs"
	"net/http"
	"strings"
)

//go:embed dist
var dist embed.FS

func Handler() http.Handler {
	sub, err := fs.Sub(dist, "dist")
	if err != nil {
		return http.NotFoundHandler()
	}
	fileServer := http.FileServer(http.FS(sub))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Serve static assets directly by extension or assets path
		if strings.HasPrefix(r.URL.Path, "/assets/") || strings.HasSuffix(r.URL.Path, ".js") || strings.HasSuffix(r.URL.Path, ".css") || strings.HasSuffix(r.URL.Path, ".svg") || strings.HasSuffix(r.URL.Path, ".ico") || strings.HasSuffix(r.URL.Path, ".png") || strings.HasSuffix(r.URL.Path, ".jpg") || strings.HasSuffix(r.URL.Path, ".txt") || strings.HasSuffix(r.URL.Path, ".map") {
			fileServer.ServeHTTP(w, r)
			return
		}
		// Always serve index.html for app routes to avoid directory redirects
		b, err := fs.ReadFile(sub, "index.html")
		if err != nil {
			http.Error(w, "index not found", http.StatusNotFound)
			return
		}
		// set content-type
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		// avoid redirects by serving content directly
		// (avoid using FileServer for index route)
		// include small cache busting header for index
		w.Header().Set("Cache-Control", "no-cache")
		// Set status code explicitly before writing
		w.WriteHeader(http.StatusOK)
		// Write the HTML
		_, _ = w.Write(b)
	})
}
