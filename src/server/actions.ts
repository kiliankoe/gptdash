"use server";

import { appState } from "./state";

export async function getGameState(id: string) {
  return appState.games.find((game) => game.id === id);
}

export async function addPlayer(name: string, gameId: string) {
  const player = { id: crypto.randomUUID(), name };
  console.log("Adding player", name);
  appState.games.find((game) => game.id === gameId)?.players.push(player);
  console.log(appState.games);
}

export async function startGame() {
  console.log("Starting game");
  appState.games[0]!.status = "waitingForPrompt";
}

export async function setPrompt(prompt: string) {
  console.log("Setting prompt", prompt);
  appState.games[0]!.status = "prompting";
  appState.games[0]!.currentPrompt = prompt;
}
