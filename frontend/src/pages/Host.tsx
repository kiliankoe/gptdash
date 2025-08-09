import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getSocket } from "../lib/socket";
import { useGameStore } from "../store/useGameStore";

export default function Host() {
  const { code } = useParams();
  const navigate = useNavigate();
  const { phase, players, round, you } = useGameStore((s) => ({
    phase: s.phase,
    players: s.players,
    round: s.round,
    you: s.you,
  }));
  const [prompt, setPrompt] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [submissionCount, setSubmissionCount] = useState(0);
  const [voteCount, setVoteCount] = useState(0);
  const [aiAnswer, setAiAnswer] = useState<string | null>(null);
  const [playerSubmissionStatus, setPlayerSubmissionStatus] = useState<Record<string, boolean>>({});

  // GM form state (for session creation)
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [provider, setProvider] = useState<string>((import.meta.env.VITE_DEFAULT_PROVIDER as string) || "openai");
  const [model, setModel] = useState<string>(
    (import.meta.env.VITE_DEFAULT_MODEL as string) || (provider === "ollama" ? "mistral" : "gpt-3.5-turbo"),
  );
  const [roundCount, setRoundCount] = useState(3);

  // Check if host has valid session token
  useEffect(() => {
    const sessionCode = localStorage.getItem("sessionCode");
    const hostToken = localStorage.getItem("hostToken");

    // If we have a code in URL but no matching session, show create form
    if (code && (!sessionCode || !hostToken || sessionCode !== code)) {
      console.log("[Host] No valid host session found, showing create form");
      setShowCreateForm(true);
      return;
    }

    // If no code in URL and no session, also show create form
    if (!code && (!sessionCode || !hostToken)) {
      console.log("[Host] No session code provided, showing create form");
      setShowCreateForm(true);
      return;
    }

    // Valid session exists
    setShowCreateForm(false);
  }, [code]);

  useEffect(() => {
    const sock = getSocket();
    sock.on("game:state", (payload: any) => {
      const { phase, players, round, you, sessionCode } = payload;
      useGameStore.getState().setState({ phase, players, round, you, sessionCode });
    });
    sock.on("game:submissions", (payload: any) => {
      setSubmissionCount(payload.count || 0);
      setPlayerSubmissionStatus(payload.playerStatus || {});
    });
    sock.on("game:results", (payload: any) => {
      // Extract AI answer from results
      if (payload.aiSubmissionId && payload.submissions) {
        const aiSubmission = payload.submissions.find((s: any) => s.id === payload.aiSubmissionId);
        if (aiSubmission) {
          setAiAnswer(aiSubmission.text);
        }
      }
    });
    sock.on("game:aiAnswer", (payload: any) => {
      // Set AI answer as soon as it's ready
      if (payload.answer) {
        setAiAnswer(payload.answer);
      }
    });
    sock.on("game:votes", (payload: any) => {
      setVoteCount(payload.count || 0);
    });
    // Reset vote count when entering new phases
    if (phase === "Answering") {
      setVoteCount(0);
      setAiAnswer(null); // Reset AI answer for new round
      setSubmissionCount(0);
      setPlayerSubmissionStatus({});
    }
    if (phase === "Voting") {
      setVoteCount(0);
    }
    return () => {
      sock.off("game:state");
      sock.off("game:submissions");
      sock.off("game:results");
      sock.off("game:aiAnswer");
      sock.off("game:votes");
    };
  }, [phase]);

  const onCreate = async () => {
    const res = await fetch("/api/gm/create", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        config: { provider, model, roundCount, answerTime: 0, voteTime: 0 },
      }),
    });
    if (!res.ok) {
      alert("Fehler beim Erstellen der Session. Überprüfe Zugangsdaten und Server-Logs.");
      return;
    }
    const j = await res.json();
    const { sessionCode, hostToken } = j;
    localStorage.setItem("hostToken", hostToken);
    localStorage.setItem("sessionCode", sessionCode);
    localStorage.setItem("role", "host");
    // bind socket as host
    const sock = getSocket();
    const resume = () => sock.emit("game:resume", { sessionCode, role: "host", token: hostToken });
    if (sock.connected) resume();
    else sock.once("connect", resume);
    // Navigate to host page with the session code, or just hide the form if we're already there
    if (!code) {
      navigate(`/host/${sessionCode}`);
    } else {
      setShowCreateForm(false);
    }
  };

  const onSetPrompt = () => {
    const sock = getSocket();
    let done = false;
    const to = setTimeout(() => {
      if (!done) console.warn("setPrompt ack timeout");
    }, 5000);
    sock.emit("game:setPrompt", { prompt }, (res: any) => {
      done = true;
      clearTimeout(to);
      if (res?.error) {
        console.warn("setPrompt error", res.error);
        setMsg("Fehler: " + res.error);
      } else {
        setMsg("Frage gesetzt.");
        setPrompt(""); // Clear prompt after setting
        setSubmissionCount(0); // Reset counters
      }
    });
  };
  const onAdvance = () => {
    const sock = getSocket();

    // If we're in lobby or scoreboard and have a prompt, set it first before advancing
    if ((phase === "Lobby" || phase === "Scoreboard") && prompt.trim()) {
      let done = false;
      const to = setTimeout(() => {
        if (!done) console.warn("setPrompt ack timeout");
      }, 5000);
      sock.emit("game:setPrompt", { prompt }, (res: any) => {
        done = true;
        clearTimeout(to);
        if (res?.error) {
          console.warn("setPrompt error", res.error);
          setMsg("Fehler: " + res.error);
        } else {
          setMsg("Spiel gestartet!");
          setPrompt(""); // Clear prompt after setting
          setSubmissionCount(0); // Reset counters
          // Don't call advance here - setPrompt already transitions to Answering
        }
      });
    } else {
      // Normal advance for other phases
      let done = false;
      const to = setTimeout(() => {
        if (!done) console.warn("advance ack timeout");
      }, 5000);
      sock.emit("game:advance", (res: any) => {
        done = true;
        clearTimeout(to);
        if (res?.error) {
          console.warn("advance error", res.error);
          setMsg("Fehler: " + res.error);
        } else {
          setMsg("Nächste Phase.");
        }
      });
    }
  };
  const getPhaseDisplayName = (phase: string) => {
    switch (phase) {
      case "Lobby":
        return "Wartebereich";
      case "PromptSet":
        return "Frage vorbereiten";
      case "Answering":
        return "Antworten sammeln";
      case "Voting":
        return "Abstimmung";
      case "Scoreboard":
        return "Ergebnisse";
      case "End":
        return "Spiel beendet";
      default:
        return phase;
    }
  };

  const shouldShowPromptInput = phase === "Lobby" || phase === "PromptSet" || phase === "Scoreboard";

  // Show create form if no valid session
  if (showCreateForm) {
    return (
      <div>
        <h2>Spielleiter</h2>
        <div style={{ display: "grid", gap: 12, maxWidth: 480 }}>
          <label>
            Anbieter
            <select value={provider} onChange={(e) => setProvider(e.target.value)} style={{ marginLeft: 8 }}>
              <option value="openai">OpenAI</option>
              <option value="ollama">Ollama</option>
            </select>
          </label>
          <label>
            Modell
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={provider === "ollama" ? "mistral" : "gpt-3.5-turbo"}
              style={{ marginLeft: 8 }}
            />
          </label>
          <label>
            Runden
            <input
              type="number"
              min={1}
              max={20}
              value={roundCount}
              onChange={(e) => setRoundCount(parseInt(e.target.value || "1"))}
              style={{ marginLeft: 8, width: 100 }}
            />
          </label>
          <button type="button" onClick={onCreate}>
            Session erstellen
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <div className="card">
        <h2>Spielleiter Konsole</h2>
        {msg && (
          <div className="subtle" style={{ marginBottom: 8 }}>
            {msg}
          </div>
        )}

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: 16,
            marginBottom: 16,
          }}
        >
          <div>
            <strong>Aktuelle Phase:</strong>
            <div style={{ color: "var(--yellow)", fontSize: "1.2em" }}>{getPhaseDisplayName(phase)}</div>
          </div>
          <div>
            <strong>Mitspielende:</strong>
            <div>{players.length}</div>
          </div>
        </div>

        {round && (
          <div style={{ marginBottom: 16 }}>
            <strong>Aktuelle Frage:</strong>
            <div style={{ fontStyle: "italic", color: "var(--subtle)" }}>{round.prompt}</div>
          </div>
        )}
      </div>

      {phase === "Answering" && (
        <div className="card">
          <h3>Antworten sammeln</h3>
          <div style={{ marginBottom: 12 }}>
            <strong>Eingegangene Antworten:</strong> {submissionCount} / {players.length}
          </div>
          <div
            style={{
              background: "var(--bg-subtle)",
              padding: 8,
              borderRadius: 4,
              marginBottom: 12,
            }}
          >
            {players.map((p) => {
              const hasSubmitted = playerSubmissionStatus[p.id] || false;
              return (
                <div key={p.id} style={{ marginBottom: 4 }}>
                  {p.name}:{" "}
                  <span
                    style={{
                      color: hasSubmitted ? "var(--green)" : "var(--subtle)",
                    }}
                  >
                    {hasSubmitted ? "✓ abgegeben" : "⏳ wartend"}
                  </span>
                </div>
              );
            })}
          </div>
          {aiAnswer && (
            <div
              style={{
                background: "var(--green)",
                color: "white",
                padding: 12,
                borderRadius: 8,
              }}
            >
              <strong>🤖 KI-Antwort bereit:</strong>
              <div style={{ marginTop: 8, fontStyle: "italic" }}>"{aiAnswer}"</div>
            </div>
          )}
        </div>
      )}

      {phase === "Voting" && (
        <div className="card">
          <h3>Abstimmung läuft</h3>
          <div style={{ marginBottom: 12 }}>
            <strong>Abgegebene Stimmen:</strong> {voteCount} / {players.length}
          </div>
          <p className="subtle">
            {voteCount >= players.length && players.length > 0
              ? "Alle Spieler:innen haben abgestimmt."
              : "Die Spieler:innen stimmen gerade über die Antworten ab..."}
          </p>
        </div>
      )}

      <div className="card">
        <h3>Aktionen</h3>
        {shouldShowPromptInput && (
          <div style={{ marginBottom: 16 }}>
            <label htmlFor="prompt-textarea" style={{ display: "block", marginBottom: 8, fontWeight: "bold" }}>
              {phase === "Lobby"
                ? "Erste Frage für das Spiel:"
                : phase === "Scoreboard"
                  ? "Frage für nächste Runde:"
                  : "Neue Frage eingeben:"}
            </label>
            <textarea
              id="prompt-textarea"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="Frage eingeben..."
              rows={3}
              style={{
                width: "100%",
                maxWidth: "100%",
                boxSizing: "border-box",
                marginBottom: 12,
                resize: "vertical",
              }}
            />
            {phase === "PromptSet" && (
              <button type="button" onClick={onSetPrompt} disabled={!prompt.trim()} style={{ marginRight: 12 }}>
                Frage setzen
              </button>
            )}
          </div>
        )}
        <button
          type="button"
          onClick={onAdvance}
          disabled={(phase === "Lobby" || phase === "Scoreboard") && !prompt.trim()}
          style={{
            background:
              (phase === "Lobby" || phase === "Scoreboard") && !prompt.trim() ? "var(--subtle)" : "var(--yellow)",
            color: "var(--bg)",
            fontWeight: "bold",
            padding: "12px 24px",
            cursor: (phase === "Lobby" || phase === "Scoreboard") && !prompt.trim() ? "not-allowed" : "pointer",
          }}
          title={
            (phase === "Lobby" || phase === "Scoreboard") && !prompt.trim() ? "Bitte gib zuerst eine Frage ein" : ""
          }
        >
          {phase === "Lobby" ? "Spiel starten" : phase === "Scoreboard" ? "Nächste Runde" : "Nächste Phase"}
        </button>
      </div>
    </div>
  );
}
