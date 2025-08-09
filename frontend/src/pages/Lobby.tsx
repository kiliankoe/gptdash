import { useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { getSocket } from "../lib/socket";
import { useGameStore } from "../store/useGameStore";

export default function Lobby() {
  const { code } = useParams();
  const nav = useNavigate();
  const players = useGameStore((s) => s.players);
  const phase = useGameStore((s) => s.phase);

  // Check if player has valid session token
  useEffect(() => {
    const sessionCode = localStorage.getItem("sessionCode");
    const playerToken = localStorage.getItem("playerToken");
    const playerId = localStorage.getItem("playerId");

    // If no session or token, or if session doesn't match current code, redirect to home with session code
    if (!sessionCode || !playerToken || sessionCode !== code) {
      console.log("[Lobby] No valid session found, redirecting to home with session code");
      nav(`/?join=${code}`);
      return;
    }

    // If we have tokens but no playerId, we're in a broken state (likely from refresh)
    // The socket resume should fix this, but if it doesn't work, we'll redirect
    if (!playerId) {
      console.log("[Lobby] Missing playerId, waiting for resume or will redirect");
      const timeout = setTimeout(() => {
        // If still no playerId after 3 seconds, assume resume failed
        if (!localStorage.getItem("playerId")) {
          console.log("[Lobby] Resume failed, redirecting to home");
          nav(`/?join=${code}`);
        }
      }, 3000);
      return () => clearTimeout(timeout);
    }
  }, [code, nav]);

  useEffect(() => {
    const sock = getSocket();
    sock.on("game:state", (payload: any) => {
      const { phase, players, round, you, sessionCode } = payload;
      console.log("[Lobby] Received game:state:", {
        phase,
        playersCount: players?.length,
        you: you?.role,
        playerId: you?.playerId,
      });

      // Restore playerId if missing (happens after page refresh)
      if (you?.playerId && !localStorage.getItem("playerId")) {
        console.log("[Lobby] Restoring playerId from game state:", you.playerId);
        localStorage.setItem("playerId", you.playerId);
      }

      useGameStore.getState().setState({ phase, players, round, you, sessionCode });
    });
    return () => {
      sock.off("game:state");
    };
  }, []);

  // Auto-navigate when game starts
  useEffect(() => {
    if (phase !== "Lobby" && phase !== "PromptSet") {
      nav(`/play/${code}`);
    }
  }, [phase, code, nav]);

  return (
    <div>
      <h2>Lobby</h2>

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
          Phase: {phase} | Players: {players.length} | Data:{" "}
          {JSON.stringify(players.map((p) => ({ id: p.id, name: p.name })))}
        </div>
      )}

      <div className="card">
        <h3>Spieler:in ({players.length})</h3>
        {players.length > 0 ? (
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {players.map((p, index) => (
              <li
                key={p.id || index}
                style={{
                  padding: "8px 0",
                  borderBottom: index < players.length - 1 ? "1px solid var(--border)" : "none",
                }}
              >
                👤 <strong>{p.name || p.id || `Spieler:in ${index + 1}`}</strong>
                {p.isHost && <span style={{ marginLeft: 8, color: "var(--yellow)" }}>(Spielleiter:in)</span>}
              </li>
            ))}
          </ul>
        ) : (
          <p style={{ color: "var(--subtle)", fontStyle: "italic" }}>Noch keine Mitspielenden verbunden...</p>
        )}
      </div>

      {phase === "PromptSet" && (
        <p style={{ color: "var(--yellow)", marginTop: 16 }}>Spielleiter:in bereitet eine neue Runde vor...</p>
      )}
    </div>
  );
}
