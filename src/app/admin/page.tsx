"use client";

import Button from "../components/Button";
import { useGame } from "../components/GameProvider";
import PlayerList from "../components/PlayerList";
import ChoosePrompt from "./ChoosePrompt";

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
            onClick={() =>
              fetch("/api/game/ds24/status", {
                method: "POST",
                body: JSON.stringify({ status: "waitingForPrompt" }),
              })
            }
          >
            Runde starten
          </Button>
        </div>
      );
    case "waitingForPrompt":
      return <ChoosePrompt />;
    case "prompting":
      return <div>Prompting</div>;
    case "voting":
      return <div>Voting</div>;
    case "leaderboard":
      return <div>Leaderboard</div>;
  }
}
