"use client";

import { useGame } from "~/app/components/GameProvider";
import Prompting from "./Prompting";
import Voting from "./Voting";
import { WaitingToStart } from "./WaitingToStart";

export default function GamePage({}: { params: { gameId: string } }) {
  const { game, isLoading } = useGame();
  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game) {
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;
  }
  switch (game.status) {
    case "waitingToStart":
      return <WaitingToStart />;
    case "waitingForPrompt":
      return <div>Prompt wird ausgewählt! Gleich geht&apos;s weiter!</div>;
    case "prompting":
      return <Prompting />;
    case "voting":
      return <Voting />;
    case "leaderboard":
      return <div>Leaderboard</div>;
  }
}
