"use client";

import { createContext, useContext } from "react";
import { useQuery } from "react-query";
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
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    data: game,
    error,
    isLoading,
  } = useQuery({
    queryKey: ["gamestate", "ds24"],
    queryFn: () => getGameState("ds24"),
  });
  return (
    // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
    <GameContext.Provider value={{ game, error, isLoading }}>
      {children}
    </GameContext.Provider>
  );
}

export function useGame() {
  return useContext(GameContext);
}
