package ws

import (
    "context"
    "net/http"
    "strings"

    "github.com/gin-gonic/gin"
    socketio "github.com/googollee/go-socket.io"
    "github.com/kiliankoe/gptdash/internal/config"
    "github.com/kiliankoe/gptdash/internal/game"
    "github.com/rs/zerolog/log"
)

type ConnCtx struct {
    Code  string
    Token string
    Role  string // "host" | "player"
}

type Server struct {
    RM           *game.RoomManager
    members      map[string]map[string]socketio.Conn // sessionCode -> socketID -> Conn
    provider     AIProvider
    provByName   map[string]AIProvider
    systemPrompt string
    config       config.Config
}

type AIProvider interface {
    Complete(ctx context.Context, model string, prompt string) (string, error)
    CompleteWithSystem(ctx context.Context, model string, systemPrompt string, prompt string) (string, error)
}

func New(rm *game.RoomManager, cfg config.Config) *Server {
    return &Server{RM: rm, members: make(map[string]map[string]socketio.Conn), config: cfg}
}

func (srv *Server) SetProvider(p AIProvider) { srv.provider = p }
func (srv *Server) SetProviders(m map[string]AIProvider) { srv.provByName = m }
func (srv *Server) SetSystemPrompt(prompt string) { srv.systemPrompt = prompt }

// Mount attaches Socket.IO server with handlers to the given Gin engine.
func (srv *Server) Mount(r *gin.Engine) *socketio.Server {
    io := socketio.NewServer(nil)

    io.OnConnect("/", func(s socketio.Conn) error {
        s.SetContext(&ConnCtx{})
        log.Info().Str("sid", s.ID()).Msg("socket connected")
        return nil
    })

    // game:create
    io.OnEvent("/", "game:create", func(s socketio.Conn, payload struct {
        Config game.SessionConfig `json:"config"`
    }) map[string]any {
        code, hostToken, _ := srv.RM.CreateSession(payload.Config)
        s.SetContext(&ConnCtx{Code: code, Token: hostToken, Role: "host"})
        s.Join(code)
        srv.addMember(code, s)
        log.Info().Str("sid", s.ID()).Str("code", code).Msg("game:create")
        // send initial state to host only
        srv.emitStateTo(code)
        return map[string]any{"sessionCode": code, "hostToken": hostToken}
    })

    // game:join
    io.OnEvent("/", "game:join", func(s socketio.Conn, payload struct {
        SessionCode string `json:"sessionCode"`
        Name        string `json:"name"`
    }) map[string]any {
        sess, err := srv.RM.Get(payload.SessionCode)
        if err != nil {
            return srv.err(s, "session_not_found", "Session not found")
        }
        playerID, playerToken := sess.Join(payload.Name)
        s.SetContext(&ConnCtx{Code: payload.SessionCode, Token: playerToken, Role: "player"})
        s.Join(payload.SessionCode)
        srv.addMember(payload.SessionCode, s)
        log.Info().Str("sid", s.ID()).Str("code", payload.SessionCode).Str("playerId", playerID).Msg("game:join")
        // broadcast updated state to all in room (personalized per-conn)
        srv.emitStateTo(payload.SessionCode)
        return map[string]any{"playerToken": playerToken, "playerId": playerID}
    })

    // game:resume (reconnection)
    io.OnEvent("/", "game:resume", func(s socketio.Conn, payload struct {
        SessionCode string `json:"sessionCode"`
        Role        string `json:"role"`
        Token       string `json:"token"`
    }) map[string]any {
        sess, err := srv.RM.Get(payload.SessionCode)
        if err != nil { return srv.err(s, "session_not_found", "Session not found") }
        if payload.Role == "host" {
            if payload.Token != sess.HostToken { return srv.err(s, "unauthorized", "Invalid host token") }
        } else {
            id := sess.GetPlayerIDByToken(payload.Token)
            if id == "" { return srv.err(s, "unauthorized", "Invalid player token") }
        }
        s.SetContext(&ConnCtx{Code: payload.SessionCode, Token: payload.Token, Role: payload.Role})
        s.Join(payload.SessionCode)
        srv.addMember(payload.SessionCode, s)
        log.Info().Str("sid", s.ID()).Str("code", payload.SessionCode).Str("role", payload.Role).Msg("game:resume")
        // send state to only this connection
        sess2, _ := srv.RM.Get(payload.SessionCode)
        ctx := s.Context().(*ConnCtx)
        you := map[string]any{"role": ctx.Role}
        if ctx.Role == "player" {
            if id := sess2.GetPlayerIDByToken(ctx.Token); id != "" {
                you["playerId"] = id
            }
        }
        payloadOut := map[string]any{
            "phase":       string(sess2.Phase),
            "players":     sess2.Players(),
            "round":       currentRoundPtr(sess2),
            "you":         you,
            "sessionCode": payload.SessionCode,
            "scores":      sess2.ScoresArray(),
        }
        s.Emit("game:state", payloadOut)
        // Also broadcast updated state to all other connections (they need to see this player is back)
        srv.emitStateTo(payload.SessionCode)
        return map[string]any{"ok": true}
    })

    // game:setPrompt (host)
    io.OnEvent("/", "game:setPrompt", func(s socketio.Conn, payload struct {
        Prompt string `json:"prompt"`
    }) map[string]any {
        ctx := s.Context().(*ConnCtx)
        sess, err := srv.RM.Get(ctx.Code)
        if err != nil { return srv.err(s, "session_not_found", "Session not found") }
        if err := sess.SetPrompt(ctx.Token, payload.Prompt); err != nil {
            return srv.err(s, "bad_request", err.Error())
        }
        log.Info().Str("code", ctx.Code).Msg("game:setPrompt")
        // moving to Answering -> notify players
        srv.emitStateTo(ctx.Code)
        // kick off AI completion in background (best-effort)
        go func(code string) {
            // pick provider per session
            prov := srv.provider
            if srv.provByName != nil {
                if p := srv.provByName[strings.ToLower(sess.Config.Provider)]; p != nil {
                    prov = p
                }
            }
            if prov == nil { return }
            // use session config model if present
            model := sess.Config.Model
            if model == "" { model = "gpt-3.5-turbo" }
            var text string
            var err error
            if srv.systemPrompt != "" {
                text, err = prov.CompleteWithSystem(context.Background(), model, srv.systemPrompt, payload.Prompt)
            } else {
                text, err = prov.Complete(context.Background(), model, payload.Prompt)
            }
            if err == nil && text != "" {
                // insert AI submission
                _, _ = sess.AddAISubmission(text)
                // notify GM that AI answer is ready
                for _, c := range srv.members[code] {
                    if ctx, ok := c.Context().(*ConnCtx); ok && ctx.Role == "host" {
                        c.Emit("game:aiAnswer", map[string]any{"answer": text})
                    }
                }
            }
        }(ctx.Code)
        return map[string]any{"ok": true}
    })

    // game:submit
    io.OnEvent("/", "game:submit", func(s socketio.Conn, payload struct {
        Text string `json:"text"`
    }) map[string]any {
        ctx := s.Context().(*ConnCtx)
        sess, err := srv.RM.Get(ctx.Code)
        if err != nil { return srv.err(s, "session_not_found", "Session not found") }
        id, err := sess.Submit(ctx.Token, payload.Text)
        if err != nil { return srv.err(s, "bad_request", err.Error()) }
        log.Info().Str("code", ctx.Code).Str("submissionId", id).Msg("game:submit")
        // notify count update (only human submissions) and player status
        cnt := sess.HumanSubmissionCount()
        status := sess.PlayerSubmissionStatus()
        io.BroadcastToRoom("/", ctx.Code, "game:submissions", map[string]any{"count": cnt, "playerStatus": status})
        return map[string]any{"submissionId": id}
    })

    // game:advance
    io.OnEvent("/", "game:advance", func(s socketio.Conn) map[string]any {
        ctx := s.Context().(*ConnCtx)
        sess, err := srv.RM.Get(ctx.Code)
        if err != nil { return srv.err(s, "session_not_found", "Session not found") }
        // capture phase before advance to decide what to emit
        previousPhase := sess.GetPhase()
        if err := sess.Advance(ctx.Token); err != nil { return srv.err(s, "bad_request", err.Error()) }
        currentPhase := sess.GetPhase()
        log.Info().Str("code", ctx.Code).Str("from", string(previousPhase)).Str("to", string(currentPhase)).Msg("phase transition")
        
        // Export game data if we just entered Scoreboard phase (round complete)
        if currentPhase == game.PhaseScoreboard && srv.config.ExportEnabled {
            if exportErr := game.ExportSession(sess, srv.config.ExportFile); exportErr != nil {
                log.Error().Err(exportErr).Str("code", ctx.Code).Msg("failed to export game data")
            } else {
                log.Info().Str("code", ctx.Code).Str("file", srv.config.ExportFile).Msg("exported game data")
            }
        }
        log.Info().Str("code", ctx.Code).Msg("game:advance")
        // Emit state update
        srv.emitStateTo(ctx.Code)
        // If now in Voting, emit shuffled submissions
        subs := sess.ListVotingSubmissionsShuffled()
        if len(subs) > 0 {
            list := make([]map[string]any, 0, len(subs))
            for _, ssub := range subs {
                list = append(list, map[string]any{"id": ssub.ID, "text": ssub.Text})
            }
            io.BroadcastToRoom("/", ctx.Code, "game:voting", map[string]any{"submissions": list})
        }
        // If now in Scoreboard, emit results with submissions and authors
        votes := sess.Votes()
        r := currentRoundPtr(sess)
        aiID := ""
        if r != nil { aiID = r.AISubmissionID }
        // collect submissions
        subs = sess.ListVotingSubmissionsShuffled()
        // we want authors; rebuild directly from map
        // Note: ListVotingSubmissionsShuffled returns a copy without authors; we will build here with authors
        // We already have sess.submissions but it's private; use ListVotingSubmissionsShuffled then enrich via lookup
        // For simplicity, send shuffled texts and ids only (authorId best-effort)
        resultsList := make([]map[string]any, 0, len(subs))
        for _, sub := range subs {
            resultsList = append(resultsList, map[string]any{
                "id": sub.ID,
                "text": sub.Text,
                "authorId": sub.PlayerID,
            })
        }
        io.BroadcastToRoom("/", ctx.Code, "game:results", map[string]any{
            "aiSubmissionId": aiID,
            "votes": votes,
            "scores": sess.ScoresArray(),
            "submissions": resultsList,
        })
        return map[string]any{"ok": true}
    })

    // game:vote
    io.OnEvent("/", "game:vote", func(s socketio.Conn, payload struct {
        SubmissionID string `json:"submissionId"`
    }) map[string]any {
        ctx := s.Context().(*ConnCtx)
        sess, err := srv.RM.Get(ctx.Code)
        if err != nil { return srv.err(s, "session_not_found", "Session not found") }
        if err := sess.Vote(ctx.Token, payload.SubmissionID); err != nil { return srv.err(s, "bad_request", err.Error()) }
        log.Info().Str("code", ctx.Code).Str("submissionId", payload.SubmissionID).Msg("game:vote")
        // notify GM of vote count update
        voteCount := len(sess.Votes())
        io.BroadcastToRoom("/", ctx.Code, "game:votes", map[string]any{"count": voteCount})
        return map[string]any{"ok": true}
    })

    io.OnError("/", func(s socketio.Conn, e error) {
        log.Error().Str("sid", s.ID()).Err(e).Msg("socket error")
    })
    io.OnDisconnect("/", func(s socketio.Conn, reason string) {
        if ctx, ok := s.Context().(*ConnCtx); ok {
            if ctx.Code != "" {
                srv.removeMember(ctx.Code, s)
            }
        }
        log.Info().Str("sid", s.ID()).Str("reason", reason).Msg("socket disconnected")
        _ = reason
    })

    go io.Serve()

    // Mount to router
    r.GET("/socket.io/*any", gin.WrapH(io))
    r.POST("/socket.io/*any", gin.WrapH(io))

    // Basic CORS preflight for Socket.IO POST
    r.OPTIONS("/socket.io/*any", func(c *gin.Context) {
        c.Header("Access-Control-Allow-Origin", "*")
        c.Header("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
        c.Header("Access-Control-Allow-Headers", "Content-Type")
        c.Status(http.StatusNoContent)
    })

    return io
}

func (srv *Server) addMember(code string, c socketio.Conn) {
    if srv.members[code] == nil {
        srv.members[code] = make(map[string]socketio.Conn)
    }
    srv.members[code][c.ID()] = c
}

func (srv *Server) removeMember(code string, c socketio.Conn) {
    if m := srv.members[code]; m != nil {
        delete(m, c.ID())
    }
}

func (srv *Server) emitStateTo(code string) {
    sess, err := srv.RM.Get(code)
    if err != nil {
        return
    }
    for _, c := range srv.members[code] {
        ctx, _ := c.Context().(*ConnCtx)
        you := map[string]any{"role": ctx.Role}
        if ctx.Role == "player" {
            if id := sess.GetPlayerIDByToken(ctx.Token); id != "" {
                you["playerId"] = id
            }
        }
        payload := map[string]any{
            "phase":       string(sess.Phase),
            "players":     sess.Players(),
            "round":       currentRoundPtr(sess),
            "you":         you,
            "sessionCode": code,
            "scores":      sess.ScoresArray(),
        }
        c.Emit("game:state", payload)
    }
}

func (srv *Server) err(s socketio.Conn, code, message string) map[string]any {
    s.Emit("error", map[string]any{"code": code, "message": message})
    return map[string]any{"error": message}
}

func currentRoundPtr(s *game.SessionCtx) *game.Round {
    if s.RoundIx == 0 || len(s.Rounds) < s.RoundIx {
        return nil
    }
    return s.Rounds[s.RoundIx-1]
}
