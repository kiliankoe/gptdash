export type Player = {
  id: string;
  name: string;
  points: number;
};

export const gameStatuses = [
  "waitingToStart",
  "waitingForPrompt",
  "prompting",
  "voting",
  "leaderboard",
] as const;

export type GameStatus = (typeof gameStatuses)[number];

export function isGameStatus(status: string): status is GameStatus {
  return (gameStatuses as readonly string[]).includes(status);
}

export type Game = {
  id: string;
  players: Player[];
  status: GameStatus;
  rounds: Round[];
};

export type Round = {
  prompt?: string;
  aiAnswer?: string;
  submissions: Submission[];
};

export type Submission = {
  author: string;
  answer: string;
  supporters: string[]; // player IDs
};

export type AppState = {
  games: Record<string, Game>;
};

export const appState: AppState = {
  games: {
    ds24: {
      id: "ds24",
      players: [
        { id: "1", name: "kilian", points: 0 },
        { id: "2", name: "momo", points: 0 },
        { id: "3", name: "max", points: 0 },
      ],
      status: "waitingToStart",
      rounds: [],
    },
  },
};
