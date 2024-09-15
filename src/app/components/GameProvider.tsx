"use client";

import { createContext, useContext } from "react";
import { useQuery } from "react-query";

import { type Game } from "~/server/state";

export const GameContext = createContext<{
  game?: Game;
  error: unknown;
  isLoading: boolean;
}>({
  game: undefined,
  error: null,
  isLoading: true,
});

export function GameProvider({ children }: { children: React.ReactNode }) {
  const {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    data: game,
    error,
    isLoading,
  } = useQuery({
    queryKey: ["game", "ds24"],
    queryFn: () =>
      fetch("/api/game/ds24")
        .then((res) => {
          if (!res.ok) {
            console.error("Error fetching game state", res);
          }
          return res.json();
        })
        .catch((error) => {
          console.error("Error fetching game state", error);
        }),
    refetchInterval: 1000,
  });

  return (
    <GameContext.Provider
      // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
      value={{ game, error, isLoading }}
    >
      {children}
    </GameContext.Provider>
  );
}

export function useGame() {
  return useContext(GameContext);
}
