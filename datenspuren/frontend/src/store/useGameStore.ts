import { create } from "zustand";

type Phase = "Lobby" | "PromptSet" | "Answering" | "Voting" | "Reveal" | "Scoreboard" | "End";

type Player = { id: string; name: string; isHost: boolean; joinedAt: string };
type Round = {
  id: string;
  index: number;
  prompt: string;
  aiSubmissionId?: string | null;
  status: Phase;
};

type You = { role: "host" | "player"; playerId?: string };

type State = {
  sessionCode?: string;
  phase: Phase;
  players: Player[];
  round?: Round;
  you?: You;
  setState: (s: Partial<State>) => void;
};

export const useGameStore = create<State>((set) => ({
  phase: "Lobby",
  players: [],
  setState: (s) => set(s),
}));
