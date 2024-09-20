"use client";
import Image from "next/image";
import { useGame } from "~/app/components/GameProvider";
import Leaderboard from "../Leaderboard";
import WatchVoting from "./WatchVoting";

export default function WatchPage() {
  const { game, isLoading } = useGame();
  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game) {
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;
  }
  switch (game.status) {
    case "waitingToStart":
      return <GameQR />;
    case "waitingForPrompt":
      return (
        <div className="flex flex-col items-center gap-y-4">
          <h1>Prompt wird ausgewählt! Gleich geht&apos;s weiter!</h1>
          <GameQR />
        </div>
      );
    case "prompting":
      return (
        <div className="flex flex-col items-center gap-y-4">
          <h1>{game.rounds[game.rounds.length - 1]?.prompt}</h1>
          <GameQR />
        </div>
      );
    case "prevoting":
      return (
        <div className="flex flex-col items-center gap-y-4">
          <h1>Kurze Denkpause, gleich geht&apos;s weiter!</h1>
          <GameQR />
        </div>
      );
    case "voting":
      return <WatchVoting />;
    case "leaderboard":
      return <Leaderboard />;
  }
}

function GameQR() {
  return (
    <div>
      <Image
        src="/qr.png"
        alt="https://gptdash.datenspuren.de"
        width={200}
        height={200}
      />
    </div>
  );
}
