"use client";

import { useGame } from "./GameProvider";

export default function PlayerList({
  showScores = false,
}: {
  showScores?: boolean;
}) {
  const { game } = useGame();
  return (
    <div>
      <h3 className="mb-4 text-start text-xl">
        {game?.players.length} Spieler:innen:
      </h3>
      <ul className="list-inside list-disc text-start">
        {game?.players.map((player) => (
          <li key={player.id}>
            {player.name}
            {showScores && <span className="font-bold"> {player.points}</span>}
          </li>
        ))}
      </ul>
    </div>
  );
}
