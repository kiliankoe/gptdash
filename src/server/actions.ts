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

export async function submitAnswer(player: string, answer: string) {
  console.log("Submitting answer:", answer);

  // TODO: Run spell check/correction on answer?

  const game = appState.games.ds24!;
  if (!game.players.find((p) => p.id === player)) {
    throw new Error("Player not found");
  }
  const roundIndex = game.rounds.length - 1;
  if (roundIndex < 0) {
    throw new Error("No round found");
  }

  if (game.rounds[roundIndex]?.submissions.find((s) => s.author === player)) {
    game.rounds[roundIndex].submissions.find(
      (s) => s.author === player,
    )!.answer = answer;
    appState.games.ds24 = {
      ...game,
      rounds: [...game.rounds],
    };
  } else {
    game.rounds[roundIndex]?.submissions.push({
      author: player,
      answer,
      supporters: [],
    });

    appState.games.ds24 = {
      ...game,
      rounds: [...game.rounds],
    };
  }
}
