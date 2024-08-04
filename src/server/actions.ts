"use server";

import { appState } from "./state";

export async function getGameState() {
  return appState.games[0]!;
}

export async function addPlayer(name: string) {
  const player = { id: crypto.randomUUID(), name };
  appState.games[0]?.players.push(player);
}

export async function startGame() {
  appState.games[0]!.status = "waitingForPrompt";
}

export async function setPrompt(prompt: string) {
  appState.games[0]!.status = "prompting";
  appState.games[0]!.currentPrompt = prompt;
}
