"use client";

import { useGame } from "~/app/components/GameProvider";

export default function Leaderboard() {
  const { game, isLoading } = useGame();
  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game)
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;

  const players = game.players
    .filter((p) => p.name !== "AI")
    .sort((a, b) => b.points - a.points);

  const aiSubmission = game.rounds[game.rounds.length - 1]?.submissions.find(
    (s) => s.author === "AI",
  );

  return (
    <div>
      <h2>Die korrekte Antwort war</h2>
      <p>{aiSubmission?.answer}</p>
      <br />
      <h2>Richtig lagen</h2>
      <ul>
        {aiSubmission?.supporters.map((s) => (
          <li key={s}>{game.players.find((p) => p.id === s)?.name}</li>
        ))}
      </ul>
      <br />
      <h2>Leaderboard</h2>
      <ul>
        {players.map((p) => (
          <li key={p.id}>
            {p.name}: {p.points}
          </li>
        ))}
      </ul>
    </div>
  );
}
