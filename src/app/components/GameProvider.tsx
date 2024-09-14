"use client";

import { createContext, useContext } from "react";
import useSWR from "swr";
import { getGameState } from "~/server/actions";
import { appState, type Game } from "~/server/state";

export const GameContext = createContext<{
  game?: Game;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error: any;
  isLoading: boolean;
}>({
  game: appState.games[0],
  error: null,
  isLoading: false,
});

export function GameProvider({ children }: { children: React.ReactNode }) {
  const {
    data: game,
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    error,
    isLoading,
  } = useSWR("/api/gamestate", () => getGameState("ds24"), {
    refreshInterval: 500,
  });
  return (
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    <GameContext.Provider value={{ game, error, isLoading }}>
      {children}
    </GameContext.Provider>
  );
}

export function useGame() {
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
  const { game, error, isLoading } = useContext(GameContext);
  // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
  return { game, error, isLoading };
}
