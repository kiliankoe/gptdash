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

  const submissions = game.rounds[game.rounds.length - 1]?.submissions;
  
  const aiSubmission = game.rounds[game.rounds.length - 1]?.submissions.find(
    (s) => s.author === "AI",
  );

  return (
    <div>
      {submissions?.map((submission) => (
        <div key={submission.author}>
          <h2>{submission.author}</h2>
          <p>&ldquo;{submission.answer}&rdquo;</p>
          <p>Voters: {submission.supporters.map((supporter) => (
            game?.players.find((p) => p.id === supporter)
          )?.name)}</p>
          <p>+ {submission.supporters.length * 100} points!</p>
        </div>
      ))}
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
