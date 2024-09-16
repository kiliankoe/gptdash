"use server";

import { appState } from "./state";

export async function getGameState(id: string) {
  const game = appState.games[id];
  if (!game) {
    throw new Error("Spiel nicht gefunden");
  }
  return game;
}

export async function addPlayer(name: string, gameId: string) {
  const player = { id: crypto.randomUUID(), name, points: 0 };
  console.log("Adding player", name, "to game", gameId);
  const game = appState.games[gameId];
  if (!game) {
    throw new Error("Spiel nicht gefunden");
  }
  if (game.players.find((player) => player.name === name)) {
    throw new Error("Eine Person mit diesem Namen existiert bereits");
  }
  appState.games[gameId] = { ...game, players: [...game.players, player] };
  return player.id;
}

export async function startGame() {
  console.log("Starting game");
  appState.games.ds24!.status = "waitingForPrompt";
}

export async function startNewRound(prompt: string) {
  console.log("Starting new round with prompt:", prompt);
  const game = appState.games.ds24!;
  appState.games.ds24 = {
    ...game,
    status: "prompting",
    rounds: [...game.rounds, { prompt, submissions: [], aiAnswer: null }],
  };
}
