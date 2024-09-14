export type Player = {
  id: string;
  name: string;
};

export type GameStatus =
  | "waitingToStart"
  | "waitingForPrompt"
  | "prompting"
  | "voting"
  | "leaderboard";

export type Game = {
  id: string;
  players: Player[];
  status: GameStatus;
  currentPrompt?: string;
};

export type AppState = {
  games: Record<string, Game>;
};

export const appState: AppState = {
  games: {
    ds24: {
      id: "ds24",
      players: [
        { id: "1", name: "kilian" },
        { id: "2", name: "momo" },
        { id: "3", name: "max" },
      ],
      status: "waitingToStart",
    },
  },
};
