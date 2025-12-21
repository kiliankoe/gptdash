package game

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// ExportSession exports the current game state to a text file
func ExportSession(s *SessionCtx, filename string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Create directory if it doesn't exist
	dir := filepath.Dir(filename)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %w", err)
	}

	// Check if file exists to determine if we need headers
	fileExists := false
	if _, err := os.Stat(filename); err == nil {
		fileExists = true
	}

	// Open file in append mode
	file, err := os.OpenFile(filename, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
	if err != nil {
		return fmt.Errorf("failed to open file: %w", err)
	}
	defer file.Close()

	// Build the export content
	var sb strings.Builder

	// Add header only for new files or first round of a new session
	if !fileExists || s.RoundIx == 1 {
		if fileExists {
			sb.WriteString("\n\n") // Add spacing between sessions
		}
		sb.WriteString(fmt.Sprintf("GPTdash Game Results - Session %s\n", s.Code))
		sb.WriteString(fmt.Sprintf("Started: %s\n", time.Now().Format("2006-01-02 15:04:05")))
		sb.WriteString(strings.Repeat("=", 50) + "\n\n")

		// Players list (only on first round)
		sb.WriteString("Players:\n")
		for _, p := range s.PlayersByID {
			sb.WriteString(fmt.Sprintf("- %s\n", p.Name))
		}
		sb.WriteString("\n")
	}

	// Export only the current round (the last one)
	if len(s.Rounds) > 0 {
		round := s.Rounds[len(s.Rounds)-1]
		sb.WriteString(fmt.Sprintf("Round %d: \"%s\"\n", round.Index, round.Prompt))
		sb.WriteString(strings.Repeat("-", 40) + "\n")

		// We have submission data for the current round
		if len(s.submissions) > 0 {
			aiSubmissionID := round.AISubmissionID

			// List all submissions
			for _, sub := range s.submissions {
				if sub.PlayerID == "AI" {
					sb.WriteString(fmt.Sprintf("- AI: \"%s\"\n", sub.Text))
				} else {
					player := s.PlayersByID[sub.PlayerID]
					if player != nil {
						sb.WriteString(fmt.Sprintf("- %s: \"%s\"\n", player.Name, sub.Text))
					}
				}
			}

			// Count votes per submission and track voters
			voteCounts := make(map[string]int)
			votersForSubmission := make(map[string][]string)
			for voterID, vote := range s.votesByVoter {
				voteCounts[vote.TargetSubmissionID]++
				// Get voter name
				voterName := "Unknown"
				if voter := s.PlayersByID[voterID]; voter != nil {
					voterName = voter.Name
				}
				votersForSubmission[vote.TargetSubmissionID] = append(votersForSubmission[vote.TargetSubmissionID], voterName)
			}

			// Show vote results
			if len(voteCounts) > 0 {
				sb.WriteString("\nVotes:\n")
				for subID, count := range voteCounts {
					sub := s.submissions[subID]
					if sub != nil {
						name := "Unknown"
						if sub.PlayerID == "AI" {
							name = "AI"
						} else if player := s.PlayersByID[sub.PlayerID]; player != nil {
							name = player.Name
						}
						voters := votersForSubmission[subID]
						sb.WriteString(fmt.Sprintf("- %s: %d vote(s) from %s\n", name, count, strings.Join(voters, ", ")))
					}
				}

				// Show who correctly identified the AI
				correctGuessers := []string{}
				for voterID, vote := range s.votesByVoter {
					if vote.TargetSubmissionID == aiSubmissionID {
						if player := s.PlayersByID[voterID]; player != nil {
							correctGuessers = append(correctGuessers, player.Name)
						}
					}
				}
				if len(correctGuessers) > 0 {
					sb.WriteString(fmt.Sprintf("\nCorrectly identified AI: %s\n", strings.Join(correctGuessers, ", ")))
				}
			}
		}

		// Current scores after this round
		if len(s.Scores) > 0 {
			sb.WriteString("\nScores after this round:\n")
			// Sort players by score for consistent output
			type playerScore struct {
				Name  string
				Score int
			}
			scores := make([]playerScore, 0, len(s.Scores))
			for playerID, score := range s.Scores {
				if player := s.PlayersByID[playerID]; player != nil {
					scores = append(scores, playerScore{Name: player.Name, Score: score})
				}
			}
			// Simple sort by score (descending)
			for i := 0; i < len(scores); i++ {
				for j := i + 1; j < len(scores); j++ {
					if scores[j].Score > scores[i].Score {
						scores[i], scores[j] = scores[j], scores[i]
					}
				}
			}
			for _, ps := range scores {
				sb.WriteString(fmt.Sprintf("- %s: %d points\n", ps.Name, ps.Score))
			}
		}

		sb.WriteString("\n")

		// Add "Game ended" marker if this is the last round
		if s.RoundIx >= s.Config.RoundCount {
			sb.WriteString(fmt.Sprintf("Game ended at %s\n", time.Now().Format("2006-01-02 15:04:05")))
			sb.WriteString(strings.Repeat("=", 50) + "\n")
		}
	}

	// Write to file
	if _, err := file.WriteString(sb.String()); err != nil {
		return fmt.Errorf("failed to write to file: %w", err)
	}

	return nil
}
