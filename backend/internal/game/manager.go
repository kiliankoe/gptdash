package game

import (
    "errors"
    "math/rand"
    "sync"
    "time"

    "github.com/google/uuid"
)

var (
    ErrSessionNotFound = errors.New("session not found")
    ErrNotHost         = errors.New("not host")
    ErrInvalidPhase    = errors.New("invalid phase for action")
    ErrAlreadyVoted    = errors.New("already voted")
)

type SessionCtx struct {
    Code      string
    CreatedAt time.Time
    Config    SessionConfig

    HostToken string

    PlayersByToken map[string]*Player
    PlayersByID    map[string]*Player

    Phase   Phase
    RoundIx int
    Rounds  []*Round

    // per round state
    submissions map[string]*Submission // submissionID -> Submission
    byPlayer    map[string]string      // playerID -> submissionID
    votesByVoter map[string]*Vote      // voterID -> Vote

    Scores map[string]int // playerID -> points

    mu sync.Mutex
}

type RoomManager struct {
    mu       sync.RWMutex
    sessions map[string]*SessionCtx
    active   string // active session code when in single-session mode
}

func NewRoomManager() *RoomManager {
    return &RoomManager{sessions: make(map[string]*SessionCtx)}
}

func (rm *RoomManager) CreateSession(cfg SessionConfig) (code string, hostToken string, err error) {
    rm.mu.Lock()
    defer rm.mu.Unlock()

    code = randomCode(5)
    for rm.sessions[code] != nil {
        code = randomCode(5)
    }
    hostToken = uuid.NewString()
    s := &SessionCtx{
        Code:          code,
        CreatedAt:     time.Now().UTC(),
        Config:        cfg,
        HostToken:     hostToken,
        PlayersByToken: make(map[string]*Player),
        PlayersByID:    make(map[string]*Player),
        Phase:         PhaseLobby,
        RoundIx:       0,
        Rounds:        []*Round{},
        submissions:   make(map[string]*Submission),
        byPlayer:      make(map[string]string),
        votesByVoter:  make(map[string]*Vote),
        Scores:        make(map[string]int),
    }

    rm.sessions[code] = s
    rm.active = code
    return code, hostToken, nil
}

func (rm *RoomManager) Get(code string) (*SessionCtx, error) {
    rm.mu.RLock()
    defer rm.mu.RUnlock()
    s := rm.sessions[code]
    if s == nil {
        return nil, ErrSessionNotFound
    }
    return s, nil
}

func (rm *RoomManager) Active() (string, *SessionCtx) {
    rm.mu.RLock()
    defer rm.mu.RUnlock()
    if rm.active == "" {
        return "", nil
    }
    return rm.active, rm.sessions[rm.active]
}

func (s *SessionCtx) StartRound(prompt string) *Round {
    s.mu.Lock()
    defer s.mu.Unlock()
    s.RoundIx++
    r := &Round{ID: uuid.NewString(), Index: s.RoundIx, Prompt: prompt, Status: PhaseAnswering}
    s.Rounds = append(s.Rounds, r)
    s.submissions = make(map[string]*Submission)
    s.byPlayer = make(map[string]string)
    s.votesByVoter = make(map[string]*Vote)
    s.Phase = PhaseAnswering
    return r
}

func (s *SessionCtx) SetPrompt(hostToken string, prompt string) error {
    s.mu.Lock()
    defer s.mu.Unlock()
    if hostToken != s.HostToken {
        return ErrNotHost
    }
    if s.Phase != PhaseLobby && s.Phase != PhasePromptSet && s.Phase != PhaseScoreboard {
        return ErrInvalidPhase
    }
    // create new round and switch to Answering
    s.RoundIx++
    r := &Round{ID: uuid.NewString(), Index: s.RoundIx, Prompt: prompt, Status: PhaseAnswering}
    s.Rounds = append(s.Rounds, r)
    s.submissions = make(map[string]*Submission)
    s.byPlayer = make(map[string]string)
    s.votesByVoter = make(map[string]*Vote)
    s.Phase = PhaseAnswering
    return nil
}

func (s *SessionCtx) Join(name string) (playerID, playerToken string) {
    s.mu.Lock()
    defer s.mu.Unlock()
    p := &Player{ID: uuid.NewString(), Name: name, IsHost: false, JoinedAt: time.Now().UTC()}
    token := uuid.NewString()
    s.PlayersByToken[token] = p
    s.PlayersByID[p.ID] = p
    return p.ID, token
}

func (s *SessionCtx) Submit(playerToken, text string) (submissionID string, err error) {
    s.mu.Lock()
    defer s.mu.Unlock()
    if s.Phase != PhaseAnswering {
        return "", ErrInvalidPhase
    }
    p := s.PlayersByToken[playerToken]
    if p == nil {
        return "", errors.New("unauthorized")
    }
    if id, ok := s.byPlayer[p.ID]; ok {
        // update existing
        s.submissions[id].Text = text
        return id, nil
    }
    id := uuid.NewString()
    sub := &Submission{ID: id, PlayerID: p.ID, Text: text}
    s.submissions[id] = sub
    s.byPlayer[p.ID] = id
    return id, nil
}

func (s *SessionCtx) Advance(hostToken string) error {
    s.mu.Lock()
    defer s.mu.Unlock()
    if hostToken != s.HostToken {
        return ErrNotHost
    }
    switch s.Phase {
    case PhaseLobby, PhasePromptSet:
        s.Phase = PhaseAnswering
    case PhaseAnswering:
        s.Phase = PhaseVoting
        if len(s.submissions) == 0 {
            // prevent getting stuck; auto-advance to Reveal
            s.Phase = PhaseReveal
            s.computeScores()
            s.Phase = PhaseScoreboard
        }
    case PhaseVoting:
        s.Phase = PhaseReveal
        s.computeScores()
        s.Phase = PhaseScoreboard
    case PhaseScoreboard:
        if s.RoundIx >= s.Config.RoundCount {
            s.Phase = PhaseEnd
        } else {
            s.Phase = PhasePromptSet
        }
    }
    return nil
}

func (s *SessionCtx) ListVotingSubmissionsShuffled() []*Submission {
    s.mu.Lock()
    defer s.mu.Unlock()
    if s.Phase != PhaseVoting && s.Phase != PhaseReveal && s.Phase != PhaseScoreboard {
        return nil
    }
    arr := make([]*Submission, 0, len(s.submissions))
    for _, sub := range s.submissions {
        arr = append(arr, sub)
    }
    rand.Shuffle(len(arr), func(i, j int) { arr[i], arr[j] = arr[j], arr[i] })
    return arr
}

func (s *SessionCtx) Vote(playerToken string, submissionID string) error {
    s.mu.Lock()
    defer s.mu.Unlock()
    if s.Phase != PhaseVoting {
        return ErrInvalidPhase
    }
    p := s.PlayersByToken[playerToken]
    if p == nil {
        return errors.New("unauthorized")
    }
    // Must have submitted an answer this round to be allowed to vote
    if _, ok := s.byPlayer[p.ID]; !ok {
        return errors.New("must_submit_before_voting")
    }
    if _, exists := s.votesByVoter[p.ID]; exists {
        return ErrAlreadyVoted
    }
    v := &Vote{ID: uuid.NewString(), VoterID: p.ID, TargetSubmissionID: submissionID}
    s.votesByVoter[p.ID] = v
    return nil
}

func (s *SessionCtx) computeScores() {
    // +2 for each vote a player's submission receives; +1 for voting AI (if AI submission known)
    // Tally votes per submission
    votesFor := map[string]int{}
    for _, v := range s.votesByVoter {
        votesFor[v.TargetSubmissionID]++
    }
    // Award +2 per vote to submission authors
    aiID := ""
    if s.RoundIx > 0 && len(s.Rounds) >= s.RoundIx {
        r := s.Rounds[s.RoundIx-1]
        aiID = r.AISubmissionID
    }
    for subID, count := range votesFor {
        sub := s.submissions[subID]
        if sub == nil {
            continue
        }
        if subID == aiID {
            // AI does not gain points
            continue
        }
        s.Scores[sub.PlayerID] += 2 * count
    }
    // Award +1 to players who voted for AI (if any)
    if aiID != "" {
        for _, v := range s.votesByVoter {
            if v.TargetSubmissionID == aiID {
                s.Scores[v.VoterID] += 1
            }
        }
    }
}

func (s *SessionCtx) Players() []*Player {
    s.mu.Lock()
    defer s.mu.Unlock()
    out := make([]*Player, 0, len(s.PlayersByID))
    for _, p := range s.PlayersByID {
        out = append(out, &Player{ID: p.ID, Name: p.Name, IsHost: p.IsHost, JoinedAt: p.JoinedAt})
    }
    return out
}

func (s *SessionCtx) SubmissionCount() int {
    s.mu.Lock()
    defer s.mu.Unlock()
    return len(s.submissions)
}

func (s *SessionCtx) HumanSubmissionCount() int {
    s.mu.Lock()
    defer s.mu.Unlock()
    count := 0
    for _, sub := range s.submissions {
        if sub.PlayerID != "AI" {
            count++
        }
    }
    return count
}

func (s *SessionCtx) PlayerSubmissionStatus() map[string]bool {
    s.mu.Lock()
    defer s.mu.Unlock()
    status := make(map[string]bool)
    for playerID := range s.PlayersByID {
        status[playerID] = false
    }
    for _, submissionID := range s.byPlayer {
        if sub := s.submissions[submissionID]; sub != nil && sub.PlayerID != "AI" {
            status[sub.PlayerID] = true
        }
    }
    return status
}

func (s *SessionCtx) GetPlayerIDByToken(token string) string {
    s.mu.Lock()
    defer s.mu.Unlock()
    p := s.PlayersByToken[token]
    if p == nil {
        return ""
    }
    return p.ID
}

func (s *SessionCtx) Votes() []*Vote {
    s.mu.Lock()
    defer s.mu.Unlock()
    out := make([]*Vote, 0, len(s.votesByVoter))
    for _, v := range s.votesByVoter {
        out = append(out, &Vote{ID: v.ID, VoterID: v.VoterID, TargetSubmissionID: v.TargetSubmissionID})
    }
    return out
}

func (s *SessionCtx) ScoresArray() []struct{ PlayerID string; Points int } {
    s.mu.Lock()
    defer s.mu.Unlock()
    out := make([]struct{ PlayerID string; Points int }, 0, len(s.Scores))
    for id, pts := range s.Scores {
        out = append(out, struct{ PlayerID string; Points int }{PlayerID: id, Points: pts})
    }
    return out
}

// AddAISubmission inserts the AI's answer for the current round and sets AISubmissionID.
func (s *SessionCtx) AddAISubmission(text string) (string, error) {
    s.mu.Lock()
    defer s.mu.Unlock()
    if s.Phase != PhaseAnswering {
        return "", ErrInvalidPhase
    }
    if s.RoundIx == 0 || len(s.Rounds) < s.RoundIx {
        return "", errors.New("no active round")
    }
    id := uuid.NewString()
    sub := &Submission{ID: id, PlayerID: "AI", Text: text}
    s.submissions[id] = sub
    // mark on current round
    s.Rounds[s.RoundIx-1].AISubmissionID = id
    return id, nil
}

func randomCode(n int) string {
    letters := []rune("ABCDEFGHJKLMNPQRSTUVWXYZ23456789")
    b := make([]rune, n)
    for i := range b {
        b[i] = letters[rand.Intn(len(letters))]
    }
    return string(b)
}
