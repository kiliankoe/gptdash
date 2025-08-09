import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getSocket } from "../lib/socket";
import { useGameStore } from "../store/useGameStore";

type ResultPayload = {
  aiSubmissionId: string;
  votes: { id: string; voterId: string; targetSubmissionId: string }[];
  scores: { PlayerID: string; Points: number }[];
  submissions: { id: string; text: string; authorId?: string | null }[];
};

export default function Play() {
  const { code } = useParams();
  const navigate = useNavigate();
  const { phase, players, round, you } = useGameStore((s) => ({
    phase: s.phase,
    players: s.players,
    round: s.round,
    you: s.you,
  }));
  const [text, setText] = useState("");
  const [currentRound, setCurrentRound] = useState<number | null>(null);
  const [submissions, setSubmissions] = useState<{ id: string; text: string }[]>([]);
  const [results, setResults] = useState<ResultPayload | null>(null);
  const [mySubmissionId, setMySubmissionId] = useState<string | null>(null);
  const [hasVoted, setHasVoted] = useState(false);
  const [votedFor, setVotedFor] = useState<string | null>(null);
  const [showSubmitFeedback, setShowSubmitFeedback] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  // Check if player has valid session token
  useEffect(() => {
    const sessionCode = localStorage.getItem("sessionCode");
    const playerToken = localStorage.getItem("playerToken");
    const playerId = localStorage.getItem("playerId");

    // If no session or token, or if session doesn't match current code, redirect to home with session code
    if (!sessionCode || !playerToken || sessionCode !== code) {
      console.log("[Play] No valid session found, redirecting to home with session code");
      navigate(`/?join=${code}`);
      return;
    }

    // If we have tokens but no playerId, we're in a broken state (likely from refresh)
    // The socket resume should fix this, but if it doesn't work, we'll redirect
    if (!playerId) {
      console.log("[Play] Missing playerId, waiting for resume or will redirect");
      const timeout = setTimeout(() => {
        // If still no playerId after 3 seconds, assume resume failed
        if (!localStorage.getItem("playerId")) {
          console.log("[Play] Resume failed, redirecting to home");
          navigate(`/?join=${code}`);
        }
      }, 3000);
      return () => clearTimeout(timeout);
    }
  }, [code, navigate]);

  useEffect(() => {
    const sock = getSocket();
    sock.on("game:voting", (payload: any) => setSubmissions(payload.submissions || []));
    sock.on("game:results", (payload: any) => setResults(payload));
    sock.on("game:state", (payload: any) => {
      const { phase, players, round, you } = payload;
      console.log("[Play] Received game:state:", {
        phase,
        playersCount: players?.length,
        round: round ? `${round.index}: "${round.prompt}"` : "null",
        you: you?.role,
        playerId: you?.playerId,
      });

      // Restore playerId if missing (happens after page refresh)
      if (you?.playerId && !localStorage.getItem("playerId")) {
        console.log("[Play] Restoring playerId from game state:", you.playerId);
        localStorage.setItem("playerId", you.playerId);
      }

      useGameStore.getState().setState({ phase, players, round, you });
    });
    return () => {
      sock.off("game:voting");
      sock.off("game:results");
      sock.off("game:state");
    };
  }, []);

  // Clear state when moving to a new round or phase
  useEffect(() => {
    if (round && round.index !== currentRound) {
      console.log(`[Play] Round changed from ${currentRound} to ${round.index} - resetting state`);
      setText("");
      setCurrentRound(round.index);
      setMySubmissionId(null);
      setHasVoted(false);
      setVotedFor(null);
      setResults(null); // Clear previous results
      setIsSubmitting(false);
      setShowSubmitFeedback(false);
    }
  }, [round, currentRound]);

  // Note: Voting state reset is handled by round changes, not phase changes
  // This prevents interference with immediate vote feedback

  const onSubmit = () => {
    const sock = getSocket();
    setIsSubmitting(true);
    let done = false;
    const to = setTimeout(() => {
      if (!done) console.warn("submit ack timeout");
      setIsSubmitting(false);
    }, 5000);
    sock.emit("game:submit", { text }, (res: any) => {
      done = true;
      clearTimeout(to);
      setIsSubmitting(false);
      if (res?.submissionId) {
        setMySubmissionId(res.submissionId);
        setShowSubmitFeedback(true);
        // Hide feedback after 2 seconds
        setTimeout(() => setShowSubmitFeedback(false), 2000);
      }
    });
  };

  const onVote = (id: string) => {
    const sock = getSocket();
    // Immediate visual feedback
    setHasVoted(true);
    setVotedFor(id);

    let done = false;
    const to = setTimeout(() => {
      if (!done) console.warn("vote ack timeout");
    }, 5000);
    sock.emit("game:vote", { submissionId: id }, (res: any) => {
      done = true;
      clearTimeout(to);
      if (res?.error) {
        // Revert on error
        console.warn("Vote error:", res.error);
        setHasVoted(false);
        setVotedFor(null);
      }
      // If no error, keep the visual feedback we already set
    });
  };

  return (
    <div>
      {/* Debug info */}
      {process.env.NODE_ENV === "development" && (
        <div
          style={{
            background: "#333",
            color: "#fff",
            padding: 8,
            fontSize: "12px",
            marginBottom: 16,
          }}
        >
          Phase: {phase} | Round: {round ? `${round.index} - "${round.prompt}"` : "null"}
        </div>
      )}

      <div style={{ marginBottom: 16 }}>
        {round?.prompt ? (
          <div className="card" style={{ background: "var(--purple)", color: "white", padding: 16 }}>
            <h3 style={{ margin: "0 0 8px 0" }}>Frage:</h3>
            <div style={{ fontSize: "1.1em", fontWeight: "normal" }}>{round.prompt}</div>
          </div>
        ) : phase === "Answering" || phase === "Voting" ? (
          <div
            className="card"
            style={{
              background: "var(--yellow)",
              color: "var(--bg)",
              padding: 16,
            }}
          >
            <strong>⏳ Warte auf Frage vom Spielleiter...</strong>
            <div style={{ fontSize: "0.9em", marginTop: 4 }}>Falls das Problem anhält, lade die Seite neu.</div>
          </div>
        ) : null}
      </div>
      {phase === "Answering" && (
        <div className="card">
          <h3>Deine Antwort</h3>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={4}
            style={{
              width: "100%",
              maxWidth: "100%",
              boxSizing: "border-box",
              marginBottom: 12,
              resize: "vertical",
            }}
            placeholder="Schreibe deine Antwort hier..."
          />
          <button
            type="button"
            onClick={onSubmit}
            disabled={!text.trim() || isSubmitting}
            style={{
              background: showSubmitFeedback ? "var(--green)" : isSubmitting ? "var(--yellow)" : undefined,
              color: showSubmitFeedback || isSubmitting ? "white" : undefined,
              cursor: !text.trim() || isSubmitting ? "not-allowed" : "pointer",
            }}
          >
            {isSubmitting
              ? "Wird gesendet..."
              : showSubmitFeedback
                ? mySubmissionId
                  ? "✓ Aktualisiert!"
                  : "✓ Gesendet!"
                : mySubmissionId
                  ? "Antwort aktualisieren"
                  : "Antwort senden"}
          </button>
        </div>
      )}
      {phase === "Voting" && (
        <div className="card">
          <h3>Stimme für eine Antwort ab</h3>
          <p className="subtle">Welche Antwort stammt wohl von der KI?</p>
          {!mySubmissionId && (
            <div
              style={{
                padding: 16,
                background: "var(--yellow)",
                color: "var(--bg)",
                borderRadius: 8,
                marginBottom: 12,
              }}
            >
              ⚠️ Du kannst leider nicht abstimmen, da du in dieser Runde keine Antwort abgegeben hast.
            </div>
          )}
          {submissions.map((s) => {
            // Check if this is the current player's submission (prevent self-voting)
            const isOwnSubmission = mySubmissionId === s.id;
            const isVotedFor = votedFor === s.id;
            const canVote = mySubmissionId && !hasVoted && !isOwnSubmission;
            return (
              <button
                type="button"
                key={s.id}
                onClick={() => canVote && onVote(s.id)}
                disabled={!mySubmissionId || isOwnSubmission || hasVoted}
                style={{
                  display: "block",
                  marginBottom: 8,
                  width: "100%",
                  textAlign: "left",
                  padding: 12,
                  opacity: !canVote || isOwnSubmission ? 0.6 : 1,
                  cursor: !canVote || isOwnSubmission ? "not-allowed" : "pointer",
                  border: isVotedFor ? "3px solid var(--green)" : undefined,
                  background: isVotedFor ? "var(--green)" : undefined,
                  color: isVotedFor ? "white" : undefined,
                }}
                title={isOwnSubmission ? "Das ist deine eigene Antwort" : ""}
              >
                {s.text}
                {isOwnSubmission && (
                  <span
                    style={{
                      marginLeft: 8,
                      fontSize: "0.9em",
                      color: "var(--yellow)",
                    }}
                  >
                    (Deine Antwort)
                  </span>
                )}
                {isVotedFor && <span style={{ marginLeft: 8, fontSize: "0.9em" }}>✓ Gewählt</span>}
              </button>
            );
          })}
        </div>
      )}
      {phase === "Scoreboard" && results && (
        <div>
          {/* Debug info for results */}
          {process.env.NODE_ENV === "development" && (
            <div
              style={{
                background: "#333",
                color: "#fff",
                padding: 8,
                fontSize: "12px",
                marginBottom: 16,
              }}
            >
              Players: {players.length} | Scores: {results.scores.length} | Submissions:{" "}
              {results.submissions?.length || 0}
              <br />
              Players: {JSON.stringify(players.map((p) => ({ id: p.id, name: p.name })))}
              <br />
              Scores:{" "}
              {JSON.stringify(
                results.scores.map((s) => ({
                  id: s.PlayerID,
                  points: s.Points,
                })),
              )}
            </div>
          )}

          <div className="card" style={{ marginBottom: 16 }}>
            <h3>Rundenergebnis</h3>
            <div style={{ marginBottom: 16 }}>
              <strong>KI-Antwort war:</strong>
              <blockquote
                style={{
                  borderLeft: "4px solid var(--green)",
                  paddingLeft: 12,
                  margin: "8px 0",
                  fontStyle: "italic",
                  background: "var(--bg-subtle)",
                  padding: 12,
                }}
              >
                {results.submissions?.find((s: ResultPayload["submissions"][number]) => s.id === results.aiSubmissionId)
                  ?.text || "(unbekannt)"}
              </blockquote>
            </div>
          </div>

          <div className="card" style={{ marginBottom: 16 }}>
            <h3>Wer hat was gewählt?</h3>
            <div style={{ display: "grid", gap: 12 }}>
              {results.submissions?.map((submission) => {
                const isAI = submission.id === results.aiSubmissionId;
                const authorPlayer = players.find((p) => p.id === submission.authorId);
                const author = isAI ? "KI" : authorPlayer?.name || submission.authorId || "Unbekannt";
                const votesForThis = results.votes.filter((v) => v.targetSubmissionId === submission.id);

                return (
                  <div
                    key={submission.id}
                    style={{
                      border: "1px solid var(--border)",
                      borderRadius: 8,
                      padding: 12,
                      background: isAI ? "var(--green-subtle)" : "var(--bg-subtle)",
                    }}
                  >
                    <div style={{ fontWeight: "bold", marginBottom: 8 }}>
                      {isAI ? "🤖 " : "👤 "}
                      {author}: "{submission.text}"
                    </div>
                    <div style={{ fontSize: "0.9em", color: "var(--subtle)" }}>
                      {votesForThis.length > 0 ? (
                        <>
                          <strong>
                            {votesForThis.length} Stimme
                            {votesForThis.length !== 1 ? "n" : ""} von:
                          </strong>{" "}
                          {votesForThis
                            .map((vote) => {
                              const voter = players.find((p) => p.id === vote.voterId);
                              return voter?.name || vote.voterId || "Unbekannt";
                            })
                            .join(", ")}
                          {isAI && <span style={{ color: "var(--green)", marginLeft: 8 }}>✓ KI richtig erkannt!</span>}
                        </>
                      ) : (
                        <em>Keine Stimmen erhalten</em>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="card">
            <h3>Aktuelle Punkte</h3>
            <div style={{ display: "grid", gap: 8 }}>
              {results.scores
                .sort((a, b) => b.Points - a.Points) // Sort by points descending
                .map((s, index) => {
                  const player = players.find((p) => p.id === s.PlayerID);
                  const displayName = player?.name || s.PlayerID || `Spieler:in ${index + 1}`;
                  return (
                    <div
                      key={s.PlayerID}
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        padding: 8,
                        borderRadius: 4,
                        background: index === 0 ? "var(--yellow-subtle)" : "var(--bg-subtle)",
                      }}
                    >
                      <span>
                        {index === 0 ? "🥇 " : `${index + 1}. `}
                        <strong>{displayName}</strong>
                        {!player && (
                          <span
                            style={{
                              color: "var(--subtle)",
                              marginLeft: 4,
                              fontSize: "0.8em",
                            }}
                          >
                            (ID: {s.PlayerID})
                          </span>
                        )}
                      </span>
                      <span style={{ fontWeight: "bold" }}>
                        {s.Points} {s.Points > 1 ? "Punkte" : "Punkt"}
                      </span>
                    </div>
                  );
                })}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
