package main

import (
    "flag"
    "fmt"
    "log"
    "net/http"
    "os"
    "strings"
    "time"

    "github.com/gin-gonic/gin"
    "github.com/kiliankoe/gptdash/internal/ai/openai"
    "github.com/kiliankoe/gptdash/internal/ai/ollama"
    "github.com/kiliankoe/gptdash/internal/config"
    "github.com/kiliankoe/gptdash/internal/game"
    "github.com/kiliankoe/gptdash/internal/ws"
    staticserver "github.com/kiliankoe/gptdash/static"
    "github.com/rs/zerolog"
    zerologlog "github.com/rs/zerolog/log"
)

var version = "dev" // Set at build time via -ldflags

func main() {
    var (
        showHelp    = flag.Bool("help", false, "Show help message")
        showVersion = flag.Bool("version", false, "Show version information")
        portFlag    = flag.String("port", "", "Port to listen on (overrides PORT env var)")
    )
    flag.BoolVar(showHelp, "h", false, "Show help message (shorthand)")
    flag.BoolVar(showVersion, "v", false, "Show version information (shorthand)")
    flag.Parse()

    if *showHelp {
        fmt.Printf(`GPTdash - Real-time AI party game

Usage: %s [options]

Options:
  -h, --help      Show this help message
  -v, --version   Show version information
  --port PORT     Port to listen on (default: 8080 or PORT env var)

Environment Variables:
  PORT                Port to listen on (default: 8080)
  DEFAULT_PROVIDER    AI provider: "openai" or "ollama" (default: openai)
  DEFAULT_MODEL       AI model to use (default: gpt-3.5-turbo)
  OPENAI_API_KEY      OpenAI API key (required for OpenAI provider)
  OPENAI_BASE_URL     Custom OpenAI API base URL (optional)
  OLLAMA_HOST         Ollama host URL (default: http://localhost:11434)
  GM_USER             GM interface username for basic auth
  GM_PASS             GM interface password for basic auth
  SINGLE_SESSION      Allow only one active session (default: true)
  EXPORT_ENABLED      Export game results to file (default: true)
  EXPORT_FILE         Path to export game results (default: ./gptdash-results.txt)

Examples:
  %s                  Start server with default settings
  %s --port 3000      Start server on port 3000
  
Visit http://localhost:8080 after starting the server.
`, os.Args[0], os.Args[0], os.Args[0])
        return
    }

    if *showVersion {
        fmt.Printf("GPTdash %s\n", version)
        return
    }

    port := *portFlag
    if port == "" {
        port = os.Getenv("PORT")
    }
    if port == "" {
        port = "8080"
    }

    zerolog.TimeFieldFormat = time.RFC3339
    cw := zerolog.ConsoleWriter{Out: os.Stdout, TimeFormat: time.RFC3339}
    zerologlog.Logger = zerologlog.Output(cw)

    gin.SetMode(gin.ReleaseMode)
    r := gin.New()
    r.Use(gin.Recovery())
    r.Use(func(c *gin.Context) {
        start := time.Now()
        c.Next()
        path := c.Request.URL.Path
        if strings.HasPrefix(path, "/socket.io") {
            return
        }
        status := c.Writer.Status()
        dur := time.Since(start)
        zerologlog.Info().Str("path", path).Int("status", status).Dur("dur", dur).Msg("http")
    })

	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"ok": true, "time": time.Now().UTC()})
	})

    cfg := config.FromEnv()

    rm := game.NewRoomManager()
    sock := ws.New(rm, cfg)
    oa := openai.New(cfg.OpenAIKey, cfg.OpenAIBaseURL)
    ol := ollama.New(cfg.OllamaHost)
    sock.SetProvider(oa) // default fallback
    sock.SetProviders(map[string]ws.AIProvider{"openai": oa, "ollama": ol})
    sock.SetSystemPrompt(cfg.SystemPrompt)
    io := sock.Mount(r)
    defer io.Close()

    // Host-protected routes (serves the SPA index behind basic auth)
    if cfg.GMUser != "" && cfg.GMPass != "" {
        auth := gin.BasicAuth(gin.Accounts{cfg.GMUser: cfg.GMPass})
        r.GET("/host", auth, func(c *gin.Context) {
            staticserver.Handler().ServeHTTP(c.Writer, c.Request)
        })
        r.GET("/host/*any", auth, func(c *gin.Context) {
            staticserver.Handler().ServeHTTP(c.Writer, c.Request)
        })
    }

    // Minimal API for active session and GM create
    r.GET("/api/session/active", func(c *gin.Context) {
        if code, sess := rm.Active(); sess != nil {
            c.JSON(http.StatusOK, gin.H{"sessionCode": code})
            return
        }
        c.Status(http.StatusNotFound)
    })
    if cfg.GMUser != "" && cfg.GMPass != "" {
        auth := gin.BasicAuth(gin.Accounts{cfg.GMUser: cfg.GMPass})
        type createReq struct{ Config game.SessionConfig `json:"config"` }
        r.POST("/api/host/create", auth, func(c *gin.Context) {
            var req createReq
            if err := c.BindJSON(&req); err != nil {
                c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_config"})
                return
            }
            code, hostToken, _ := rm.CreateSession(req.Config)
            c.JSON(http.StatusOK, gin.H{"sessionCode": code, "hostToken": hostToken})
        })
    }

    // Serve frontend (if embedded build is present) for all other routes
    r.NoRoute(func(c *gin.Context) {
        staticserver.Handler().ServeHTTP(c.Writer, c.Request)
    })

    log.Printf("listening on :%s", port)
	if err := r.Run(":" + port); err != nil {
		log.Fatal(err)
	}
}
