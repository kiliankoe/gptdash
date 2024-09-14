"use client";

import { useGame } from "~/app/components/GameProvider";

export function WaitingToStart() {
  const { game } = useGame();
  return (
    <div className="">
      <h2 className="mb-8 text-2xl">Warte auf Spielstart</h2>
      <h3 className="mb-4 text-start text-xl">
        {game?.players.length} Spieler:innen:
      </h3>
      <ul className="list-inside list-disc text-start">
        {game?.players.map((player) => <li key={player.id}>{player.name}</li>)}
      </ul>
    </div>
  );
}
