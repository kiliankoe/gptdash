"use server";

import { appState } from "./state";

export async function getGameState(id: string) {
  const game = appState.games[id];
  if (!game) {
    console.log("Game not found");
    throw new Error("Game not found");
  }
  console.log("found game", game.id);
  return game;
}

export async function addPlayer(name: string, gameId: string) {
  const player = { id: crypto.randomUUID(), name };
  console.log("Adding player", name);
  const game = appState.games[gameId];
  if (!game) {
    throw new Error("Game not found");
  }
  if (game.players.find((player) => player.name === name)) {
    throw new Error("Player with that name already exists");
  }
  appState.games[gameId] = { ...game, players: [...game.players, player] };
  return player.id;
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
