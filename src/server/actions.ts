import { respondToPrompt } from "./ai";
import { appState, type AppState } from "./state";

export async function getGameState(id: string) {
  const game = appState.games[id];
  if (!game) {
    throw new Error("Spiel nicht gefunden");
  }
  return game;
}

export async function setAppState(state: AppState) {
  if (!state.games.ds24) {
    throw new Error("Game not found");
  }
  appState.games.ds24 = {
    ...state.games.ds24,
  };
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
  let game = appState.games.ds24!;
  appState.games.ds24 = {
    ...game,
    status: "prompting",
    rounds: [...game.rounds, { prompt, submissions: [] }],
  };

  const aiAnswer = await respondToPrompt(prompt);

  // This feels kinda dirty, but we need to get the last round again.
  // And any possible submissions from very fast users.
  game = appState.games.ds24!;
  const lastRound = game.rounds[game.rounds.length - 1];
  lastRound?.submissions.push({
    author: "0",
    answer: aiAnswer,
    supporters: [],
  });
  if (!lastRound) {
    throw new Error("No last round");
  }
  appState.games.ds24 = {
    ...game,
    rounds: [
      ...game.rounds.slice(0, -1),
      {
        ...lastRound,
      },
    ],
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

export async function goToPrevoting() {
  console.log("Going to prevoting");
  const game = appState.games.ds24!;
  game.status = "prevoting";
  appState.games.ds24 = {
    ...game,
  };
}

export async function closeRound() {
  console.log("Closing round");
  const game = appState.games.ds24!;
  game.status = "voting";
  appState.games.ds24 = {
    ...game,
  };
}

export async function voteAnswer(answerAuthor: string, voteAuthor: string) {
  console.log(`${voteAuthor} votes for ${answerAuthor}`);

  const game = appState.games.ds24!;
  if (!game.players.find((p) => p.id === voteAuthor)) {
    throw new Error("Player not found");
  }
  const roundIndex = game.rounds.length - 1;
  if (roundIndex < 0) {
    throw new Error("No round found");
  }
  const answerAuthorId = game.players.find((p) => p.name === answerAuthor)?.id;
  if (!answerAuthorId) {
    throw new Error("Answer author not found");
  }
  if (
    !game.rounds[roundIndex]?.submissions.find(
      (s) => s.author === answerAuthorId,
    )
  ) {
    throw new Error("Answer not found");
  }

  if (
    game.rounds[roundIndex]?.submissions.find((s) =>
      s.supporters.includes(voteAuthor),
    )
  ) {
    throw new Error("Player already voted");
  }

  game.rounds[roundIndex]?.submissions
    .find((s) => s.author === answerAuthorId)!
    .supporters.push(voteAuthor);

  appState.games.ds24 = {
    ...game,
    rounds: [...game.rounds],
  };
}

export async function closeVoting() {
  console.log("Closing voting and calculating points");
  calculatePoints();
  const game = appState.games.ds24!;
  game.status = "leaderboard";
  appState.games.ds24 = {
    ...game,
  };
}

function calculatePoints() {
  const players = appState.games.ds24?.players;
  if (!players) return;

  const lastRound =
    appState.games.ds24?.rounds[appState.games.ds24?.rounds.length - 1];
  if (!lastRound) return;

  const submissions = lastRound.submissions;
  if (!submissions) return;

  const updatedPlayers = players.map((player) => ({ ...player }));

  submissions.forEach((s) => {
    const points = s.supporters.length;
    const player = updatedPlayers.find((p) => p.id === s.author);

    if (player && player.name !== "AI") {
      player.points += points * 100;
    }
  });

  appState.games.ds24 = {
    ...appState.games.ds24!,
    players: updatedPlayers,
  };

  const newlyupdatedPlayers = appState.games.ds24?.players.map((player) => ({
    ...player,
  }));

  const aiSubmission = submissions.find((s) => s.author === "0");
  if (aiSubmission) {
    aiSubmission.supporters.forEach((supporterId) => {
      const supporter = newlyupdatedPlayers.find((p) => p.id === supporterId);
      if (supporter) {
        supporter.points += 100;
      }
    });
  }

  console.log("Got point totals for:", newlyupdatedPlayers);

  appState.games.ds24 = {
    ...appState.games.ds24,
    players: newlyupdatedPlayers,
  };
}
