"use client";

import { useMemo, useRef } from "react";
import { useGame } from "~/app/components/GameProvider";
import type { Submission } from "~/server/state";

function shuffleArray<T>(array: T[]): T[] {
  for (let i = array.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [array[i], array[j]] = [array[j]!, array[i]!];
  }
  return array;
}

export default function WatchVoting() {
  const { game, isLoading } = useGame();

  const allSubmissions = game?.rounds[game.rounds.length - 1]?.submissions;

  const shuffledSubmissionsRef = useRef<Submission[] | null>(null);

  const submissions = useMemo(() => {
    if (!allSubmissions) return [];
    if (!shuffledSubmissionsRef.current) {
      shuffledSubmissionsRef.current = shuffleArray([...allSubmissions]);
    }
    return shuffledSubmissionsRef.current;
  }, [allSubmissions]);

  if (isLoading) return <div>Lade Spiel...</div>;
  if (!game)
    return <div>Spiel nicht gefunden, probier&apos;s mal mit F5 🤔</div>;

  return (
    <div>
      <ul className="flex flex-col gap-y-4">
        {submissions.map((s) => (
          <li key={s.author}>
            <h3>{s.answer}</h3>
          </li>
        ))}
      </ul>
    </div>
  );
}
