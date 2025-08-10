import { useEffect } from "react";
import { Outlet } from "react-router-dom";
import { getSocket } from "./lib/socket";
import { useGameStore } from "./store/useGameStore";
import "./styles.css";
import { VERSION } from "./version";

export default function App() {
  const setState = useGameStore((s) => s.setState);
  useEffect(() => {
    const s = getSocket();
    const onState = (payload: any) => {
      setState({
        sessionCode: payload.sessionCode,
        phase: payload.phase,
        players: payload.players || [],
        round: payload.round,
        you: payload.you,
      });
    };
    const onError = (e: any) => {
      console.warn("Server error:", e?.code, e?.message);
    };
    s.on("game:state", onState);
    s.on("error", onError);
    return () => {
      s.off("game:state", onState);
      s.off("error", onError);
    };
  }, [setState]);

  return (
    <div style={{ maxWidth: 800, margin: "0 auto", padding: 24 }}>
      <header
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 24,
        }}
      >
        <h1 className="title" style={{ margin: 0 }}>
          GPTdash
        </h1>
      </header>
      <Outlet />
      <div
        style={{
          position: "fixed",
          bottom: 8,
          left: 8,
          fontSize: 10,
          color: "#999",
          opacity: 0.7,
          userSelect: "none",
          pointerEvents: "none",
        }}
      >
        {VERSION}
      </div>
    </div>
  );
}
