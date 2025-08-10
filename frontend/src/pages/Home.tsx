import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { getSocket } from "../lib/socket";

export default function Home() {
  const nav = useNavigate();
  const [searchParams] = useSearchParams();
  const [name, setName] = useState("");
  const [activeCode, setActiveCode] = useState<string | null>(null);
  useEffect(() => {
    // Check if there's a join parameter in the URL
    const joinCode = searchParams.get("join");
    if (joinCode) {
      setActiveCode(joinCode);
      // Clear the URL parameter
      nav("/", { replace: true });
      return;
    }

    const tick = () => {
      fetch("/api/session/active")
        .then(async (r) => {
          if (r.ok) {
            const j = await r.json();
            setActiveCode(j.sessionCode);
          } else {
            setActiveCode(null);
          }
        })
        .catch(() => {});
    };
    tick();
    const id = setInterval(tick, 3000);
    return () => clearInterval(id);
  }, [searchParams, nav]);

  const onJoin = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // always fetch latest active session just in case page loaded before GM created it
    const r = await fetch("/api/session/active");
    if (!r.ok) return alert("Noch keine aktive Session. Bitte warte kurz.");
    const j = await r.json();
    const code: string = j.sessionCode;
    const sock = getSocket();
    let done = false;
    const to = setTimeout(() => {
      if (!done) console.warn("join ack timeout");
    }, 5000);
    sock.emit("game:join", { sessionCode: code, name }, (res: any) => {
      done = true;
      clearTimeout(to);
      if (res?.playerToken) {
        localStorage.setItem("playerToken", res.playerToken);
        localStorage.setItem("playerId", res.playerId);
        localStorage.setItem("sessionCode", code);
        localStorage.setItem("role", "player");
        nav(`/lobby/${code}`);
      } else if (res?.error) {
        console.warn("join error", res.error);
      }
    });
  };

  return (
    <div className="col" style={{ gap: 16 }}>
      <div className="card">
        <div className="title">Spiel beitreten</div>
        <p className="subtle">Gib deinen Namen ein, um der aktuellen Session beizutreten.</p>
        <form onSubmit={onJoin} className="row" style={{ marginTop: 12 }}>
          <input
            style={{ flex: 1 }}
            value={name}
            onChange={(e) => setName(e.target.value)}
            name="name"
            placeholder="Dein Name"
            required
            maxLength={40}
          />
          <button
            type="submit"
            disabled={!activeCode}
            title={!activeCode ? "Warte auf Spielleiter..." : "Spiel beitreten"}
          >
            {activeCode ? "Beitreten" : "Warte auf Spiel..."}
          </button>
        </form>
      </div>
    </div>
  );
}
