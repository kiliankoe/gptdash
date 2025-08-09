package game

import (
    "time"
)

type Phase string

const (
    PhaseLobby      Phase = "Lobby"
    PhasePromptSet  Phase = "PromptSet"
    PhaseAnswering  Phase = "Answering"
    PhaseVoting     Phase = "Voting"
    PhaseReveal     Phase = "Reveal"
    PhaseScoreboard Phase = "Scoreboard"
    PhaseEnd        Phase = "End"
)

type SessionConfig struct {
    Provider   string `json:"provider"`
    Model      string `json:"model"`
    RoundCount int    `json:"roundCount"`
    AnswerTime int    `json:"answerTime"` // seconds
    VoteTime   int    `json:"voteTime"`   // seconds
}

type Player struct {
    ID       string    `json:"id"`
    Name     string    `json:"name"`
    IsHost   bool      `json:"isHost"`
    JoinedAt time.Time `json:"joinedAt"`
}

type Round struct {
    ID             string `json:"id"`
    Index          int    `json:"index"`
    Prompt         string `json:"prompt"`
    AISubmissionID string `json:"aiSubmissionId"`
    Status         Phase  `json:"status"`
}

type Submission struct {
    ID       string `json:"id"`
    PlayerID string `json:"playerId"`
    Text     string `json:"text"`
}

type Vote struct {
    ID                string `json:"id"`
    VoterID           string `json:"voterId"`
    TargetSubmissionID string `json:"targetSubmissionId"`
}

