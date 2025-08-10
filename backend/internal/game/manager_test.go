package game

import (
	"testing"
)

func TestNewRoomManager(t *testing.T) {
	rm := NewRoomManager()
	if rm.sessions == nil {
		t.Fatal("sessions map should be initialized")
	}
	if rm.active != "" {
		t.Fatal("active session should be empty initially")
	}
}

func TestCreateSession(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{
		Provider:   "openai",
		Model:      "gpt-3.5-turbo",
		RoundCount: 3,
	}

	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}

	// Verify return values
	if code == "" {
		t.Fatal("session code should not be empty")
	}
	if hostToken == "" {
		t.Fatal("host token should not be empty")
	}

	// Verify session is stored
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to retrieve created session: %v", err)
	}

	// Verify session properties
	if session.Code != code {
		t.Fatalf("expected code %s, got %s", code, session.Code)
	}
	if session.HostToken != hostToken {
		t.Fatalf("expected host token %s, got %s", hostToken, session.HostToken)
	}
	if session.Config.Provider != "openai" {
		t.Fatalf("expected provider openai, got %s", session.Config.Provider)
	}
	if session.Phase != PhaseLobby {
		t.Fatalf("expected phase %s, got %s", PhaseLobby, session.Phase)
	}
}

func TestPlayerJoin(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 3}
	code, _, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}

	// First player joins
	playerID1, playerToken1 := session.Join("Alice")
	if playerID1 == "" {
		t.Fatal("player ID should not be empty")
	}
	if playerToken1 == "" {
		t.Fatal("player token should not be empty")
	}

	// Verify player is stored
	player := session.PlayersByID[playerID1]
	if player == nil {
		t.Fatal("player should be stored by ID")
	}
	if player.Name != "Alice" {
		t.Fatalf("expected name Alice, got %s", player.Name)
	}
	if player.IsHost {
		t.Fatal("regular player should not be host")
	}

	// Verify player is also stored by token
	if session.PlayersByToken[playerToken1] != player {
		t.Fatal("player should be stored by token")
	}

	// Second player joins
	playerID2, playerToken2 := session.Join("Bob")
	if playerID2 == playerID1 {
		t.Fatal("different players should have different IDs")
	}
	if playerToken2 == playerToken1 {
		t.Fatal("different players should have different tokens")
	}

	// Verify both players are present
	players := session.Players()
	if len(players) != 2 {
		t.Fatalf("expected 2 players, got %d", len(players))
	}
}

func TestGamePhaseTransitions(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 2}
	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}

	// Initial state
	if session.Phase != PhaseLobby {
		t.Fatalf("expected initial phase %s, got %s", PhaseLobby, session.Phase)
	}

	// Set prompt should transition directly to Answering
	err = session.SetPrompt(hostToken, "What is the capital of France?")
	if err != nil {
		t.Fatalf("should be able to set prompt: %v", err)
	}
	if session.Phase != PhaseAnswering {
		t.Fatalf("expected phase %s after setting prompt, got %s", PhaseAnswering, session.Phase)
	}

	// Verify round was created
	if len(session.Rounds) != 1 {
		t.Fatalf("expected 1 round, got %d", len(session.Rounds))
	}
	round := session.Rounds[0]
	if round.Prompt != "What is the capital of France?" {
		t.Fatalf("expected prompt in round, got %s", round.Prompt)
	}
	if round.Index != 1 {
		t.Fatalf("expected round index 1, got %d", round.Index)
	}
}

func TestSubmissionFlow(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 2}
	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}

	// Add players
	playerID1, playerToken1 := session.Join("Alice")
	playerID2, playerToken2 := session.Join("Bob")

	// Start game (SetPrompt transitions directly to Answering)
	session.SetPrompt(hostToken, "Test question?")

	// Players submit answers
	submissionID1, err := session.Submit(playerToken1, "Alice's answer")
	if err != nil {
		t.Fatalf("should be able to submit answer: %v", err)
	}
	if submissionID1 == "" {
		t.Fatal("submission ID should not be empty")
	}

	_, err = session.Submit(playerToken2, "Bob's answer")
	if err != nil {
		t.Fatalf("should be able to submit answer: %v", err)
	}

	// Verify submissions are stored
	if session.SubmissionCount() != 2 {
		t.Fatalf("expected 2 submissions, got %d", session.SubmissionCount())
	}

	// Test updating submission (same player submits again)
	newSubmissionID, err := session.Submit(playerToken1, "Alice's updated answer")
	if err != nil {
		t.Fatalf("should be able to update submission: %v", err)
	}
	if newSubmissionID != submissionID1 {
		t.Fatal("updating submission should return same ID")
	}

	// Should still have 2 submissions (updated, not added)
	if session.SubmissionCount() != 2 {
		t.Fatalf("expected 2 submissions after update, got %d", session.SubmissionCount())
	}

	// Verify submission content was updated
	submission := session.submissions[submissionID1]
	if submission.Text != "Alice's updated answer" {
		t.Fatalf("expected updated text, got %s", submission.Text)
	}

	// Verify player tracking
	status := session.PlayerSubmissionStatus()
	if !status[playerID1] {
		t.Fatal("Alice should have submitted")
	}
	if !status[playerID2] {
		t.Fatal("Bob should have submitted")
	}
}

func TestVotingFlow(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 1}
	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}

	// Add players
	_, playerToken1 := session.Join("Alice")
	_, playerToken2 := session.Join("Bob")
	_, playerToken3 := session.Join("Charlie")

	// Start game (SetPrompt transitions directly to Answering)
	session.SetPrompt(hostToken, "Test question?")

	// Players submit
	_, err = session.Submit(playerToken1, "Alice's answer")
	if err != nil {
		t.Fatalf("should be able to submit: %v", err)
	}
	submissionID2, err := session.Submit(playerToken2, "Bob's answer")
	if err != nil {
		t.Fatalf("should be able to submit: %v", err)
	}
	submissionID3, err := session.Submit(playerToken3, "Charlie's answer")
	if err != nil {
		t.Fatalf("should be able to submit: %v", err)
	}

	// Add AI answer
	aiSubmissionID, err := session.AddAISubmission("AI answer")
	if err != nil {
		t.Fatalf("should be able to add AI submission: %v", err)
	}

	session.Advance(hostToken) // To Voting

	if session.Phase != PhaseVoting {
		t.Fatalf("expected phase %s, got %s", PhaseVoting, session.Phase)
	}

	// Players vote (Alice votes for Bob's answer, Bob votes for AI)
	err = session.Vote(playerToken1, submissionID2)
	if err != nil {
		t.Fatalf("should be able to vote: %v", err)
	}

	err = session.Vote(playerToken2, aiSubmissionID)
	if err != nil {
		t.Fatalf("should be able to vote: %v", err)
	}

	// Test duplicate voting prevention
	err = session.Vote(playerToken1, submissionID3)
	if err != ErrAlreadyVoted {
		t.Fatalf("expected ErrAlreadyVoted, got %v", err)
	}

	// Note: Current implementation allows self-voting
	// This could be a design choice or a feature to implement later
	err = session.Vote(playerToken3, submissionID3)
	if err != nil {
		t.Fatalf("current implementation allows self-voting: %v", err)
	}

	votes := session.Votes()
	if len(votes) != 3 {
		t.Fatalf("expected 3 votes, got %d", len(votes))
	}
}

func TestScoringLogic(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 1}
	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}

	// Add players
	playerID1, playerToken1 := session.Join("Alice")
	playerID2, playerToken2 := session.Join("Bob")
	playerID3, playerToken3 := session.Join("Charlie")

	// Start game (SetPrompt transitions directly to Answering)
	session.SetPrompt(hostToken, "Test question?")

	// All players submit
	_, err = session.Submit(playerToken1, "Alice's answer")
	if err != nil {
		t.Fatalf("should be able to submit: %v", err)
	}
	submissionID2, err := session.Submit(playerToken2, "Bob's answer")
	if err != nil {
		t.Fatalf("should be able to submit: %v", err)
	}
	_, err = session.Submit(playerToken3, "Charlie's answer")
	if err != nil {
		t.Fatalf("should be able to submit: %v", err)
	}

	aiSubmissionID, err := session.AddAISubmission("AI answer")
	if err != nil {
		t.Fatalf("should be able to add AI submission: %v", err)
	}

	session.Advance(hostToken) // To Voting

	// Voting scenario:
	// Alice votes for Bob (+2 points to Bob)
	// Bob votes for AI (+1 point to Bob for correct guess)
	// Charlie votes for Bob (+2 points to Bob)
	session.Vote(playerToken1, submissionID2)  // Alice -> Bob
	session.Vote(playerToken2, aiSubmissionID) // Bob -> AI (correct)
	session.Vote(playerToken3, submissionID2)  // Charlie -> Bob

	session.Advance(hostToken) // To Scoreboard (triggers scoring)

	scores := session.ScoresArray()
	scoreMap := make(map[string]int)
	for _, score := range scores {
		scoreMap[score.PlayerID] = score.Points
	}

	// Bob should have: 2 votes (4 points) + 1 correct AI guess (1 point) = 5 points
	if scoreMap[playerID2] != 5 {
		t.Fatalf("expected Bob to have 5 points, got %d", scoreMap[playerID2])
	}

	// Alice and Charlie should have 0 points (no votes received, didn't guess AI)
	if scoreMap[playerID1] != 0 {
		t.Fatalf("expected Alice to have 0 points, got %d", scoreMap[playerID1])
	}
	if scoreMap[playerID3] != 0 {
		t.Fatalf("expected Charlie to have 0 points, got %d", scoreMap[playerID3])
	}
}

func TestAuthenticationAndAuthorization(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 1}
	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}

	// Test invalid host token
	err = session.SetPrompt("invalid-token", "Test question?")
	if err != ErrNotHost {
		t.Fatalf("expected ErrNotHost with invalid token, got %v", err)
	}

	err = session.Advance("invalid-token")
	if err != ErrNotHost {
		t.Fatalf("expected ErrNotHost with invalid token, got %v", err)
	}

	// Test valid host token
	err = session.SetPrompt(hostToken, "Test question?")
	if err != nil {
		t.Fatalf("should be able to set prompt with valid host token: %v", err)
	}

	// Test invalid player token for submission
	_, err = session.Submit("invalid-player-token", "Answer")
	if err == nil {
		t.Fatal("should not be able to submit with invalid player token")
	}

	// Test valid player operations (after SetPrompt we're in Answering phase)
	_, playerToken := session.Join("Alice")

	_, err = session.Submit(playerToken, "Valid answer")
	if err != nil {
		t.Fatalf("should be able to submit with valid player token: %v", err)
	}
}

func TestInvalidPhaseActions(t *testing.T) {
	rm := NewRoomManager()
	config := SessionConfig{Provider: "openai", Model: "gpt-3.5-turbo", RoundCount: 1}
	code, hostToken, err := rm.CreateSession(config)
	if err != nil {
		t.Fatalf("should be able to create session: %v", err)
	}
	session, err := rm.Get(code)
	if err != nil {
		t.Fatalf("should be able to get session: %v", err)
	}
	_, playerToken := session.Join("Alice")

	// Test submitting in wrong phase
	_, err = session.Submit(playerToken, "Answer")
	if err != ErrInvalidPhase {
		t.Fatalf("expected ErrInvalidPhase when submitting in Lobby, got %v", err)
	}

	// Test voting in wrong phase
	err = session.Vote(playerToken, "some-submission-id")
	if err != ErrInvalidPhase {
		t.Fatalf("expected ErrInvalidPhase when voting in Lobby, got %v", err)
	}

	// Move to valid phase (SetPrompt transitions directly to Answering)
	session.SetPrompt(hostToken, "Test question?")

	// Now submission should work
	_, err = session.Submit(playerToken, "Answer")
	if err != nil {
		t.Fatalf("should be able to submit in Answering phase: %v", err)
	}

	// But voting should still fail
	err = session.Vote(playerToken, "some-submission-id")
	if err != ErrInvalidPhase {
		t.Fatalf("expected ErrInvalidPhase when voting in Answering, got %v", err)
	}
}
