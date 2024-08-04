export type Player = {
  id: string;
  name: string;
};

export type Game = {
  id: string;
  players: Player[];
  status:
    | "waitingToStart"
    | "waitingForPrompt"
    | "prompting"
    | "voting"
    | "leaderboard";
  currentPrompt?: string;
};

export type AppState = {
  games: Game[];
};

export const appState: AppState = {
  games: [{ id: "ds24", players: [], status: "waitingToStart" }],
};
