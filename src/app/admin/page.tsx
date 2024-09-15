"use client";

import Button from "../components/Button";
import { useGame } from "../components/GameProvider";
import PlayerList from "../components/PlayerList";

export default function AdminPage() {
  const { game, isLoading } = useGame();
  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game) return <div>Spiel nicht gefunden lol</div>;
  switch (game.status) {
    case "waitingToStart":
      return (
        <div className="flex flex-col gap-y-4">
          <PlayerList showScores />
          <Button
            onClick={() => fetch("/api/game/ds24/status", { method: "POST" })}
          >
            Runde starten
          </Button>
        </div>
      );
    case "waitingForPrompt":
      return <div>Waiting for prompt</div>;
    case "prompting":
      return <div>Prompting</div>;
    case "voting":
      return <div>Voting</div>;
    case "leaderboard":
      return <div>Leaderboard</div>;
  }
}
