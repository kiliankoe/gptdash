package main

import (
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

func main() {
	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

    // zerolog setup (human-friendly console)
    zerolog.TimeFieldFormat = time.RFC3339
    cw := zerolog.ConsoleWriter{Out: os.Stdout, TimeFormat: time.RFC3339}
    zerologlog.Logger = zerologlog.Output(cw)

    // Gin setup with custom logger (skip /socket.io noise)
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

	// Healthcheck
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"ok": true, "time": time.Now().UTC()})
	})

    // Config
    cfg := config.FromEnv()

    // Socket server + game manager
    rm := game.NewRoomManager()
    sock := ws.New(rm)
    // Providers
    oa := openai.New(cfg.OpenAIKey, cfg.OpenAIBaseURL)
    ol := ollama.New(cfg.OllamaHost)
    sock.SetProvider(oa) // default fallback
    sock.SetProviders(map[string]ws.AIProvider{"openai": oa, "ollama": ol})
    sock.SetSystemPrompt(cfg.SystemPrompt)
    io := sock.Mount(r)
    defer io.Close()

    // GM-protected route (serves the SPA index behind basic auth)
    if cfg.GMUser != "" && cfg.GMPass != "" {
        auth := gin.BasicAuth(gin.Accounts{cfg.GMUser: cfg.GMPass})
        r.GET("/gm", auth, func(c *gin.Context) {
            staticserver.Handler().ServeHTTP(c.Writer, c.Request)
        })
        r.GET("/gm/*any", auth, func(c *gin.Context) {
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
        r.POST("/api/gm/create", auth, func(c *gin.Context) {
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
