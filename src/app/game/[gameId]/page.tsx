"use client";

import { useGame } from "~/app/components/GameProvider";
import { WaitingToStart } from "./WaitingToStart";

export default function GamePage({ params }: { params: { gameId: string } }) {
  const { game, isLoading } = useGame();
  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game) {
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;
  }
  switch (game.status) {
    case "waitingToStart":
      return <WaitingToStart />;
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
