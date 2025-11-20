/**
 * Beamer-specific JavaScript
 */

let wsConn = null;
let currentPhase = "LOBBY";
const gameState = {
  round: null,
  submissions: [],
  voteCounts: { ai: {}, funny: {} },
  scores: { players: [], audience_top: [] },
};

// Initialize on page load
function init() {
  wsConn = new WSConnection("beamer", handleMessage, updateConnectionStatus);
  wsConn.connect();
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      console.log("Welcome message:", message);
      if (message.game) {
        updatePhase(message.game.phase);
      }
      break;

    case "phase":
      updatePhase(message.phase);
      break;

    case "round_started":
      gameState.round = message.round;
      updateSceneContent(currentPhase);
      break;

    case "prompt_selected":
      if (!gameState.round) {
        gameState.round = { selected_prompt: null, prompt_candidates: [] };
      }
      gameState.round.selected_prompt = message.prompt;
      updateSceneContent(currentPhase);
      break;

    case "submissions":
      gameState.submissions = message.list || [];
      updateSubmissions();
      break;

    case "beamer_vote_counts":
      gameState.voteCounts = {
        ai: message.ai || {},
        funny: message.funny || {},
      };
      updateVoteBars();
      break;

    case "scores":
      gameState.scores = {
        players: message.players || [],
        audience_top: message.audience_top || [],
      };
      updateResults();
      break;
  }
}

function updatePhase(phase) {
  if (currentPhase === phase) return;

  console.log("Phase transition:", currentPhase, "->", phase);
  currentPhase = phase;

  // Hide all scenes
  document.querySelectorAll(".scene").forEach((scene) => {
    scene.classList.remove("active");
  });

  // Show current scene
  const sceneId = `scene-${phase}`;
  const sceneEl = document.getElementById(sceneId);
  if (sceneEl) {
    sceneEl.classList.add("active");
  } else {
    console.warn("Unknown phase:", phase);
  }

  // Update scene-specific content
  updateSceneContent(phase);
}

function updateSceneContent(phase) {
  switch (phase) {
    case "PROMPT_SELECTION":
      updatePromptSelection();
      break;
    case "WRITING":
      updateWritingScene();
      break;
    case "REVEAL":
      updateRevealScene();
      break;
    case "VOTING":
      initializeVoteBars();
      break;
    case "RESULTS":
      updateResults();
      break;
    case "PODIUM":
      updatePodium();
      break;
  }
}

function updatePromptSelection() {
  const grid = document.getElementById("promptsGrid");
  if (!grid) {
    return;
  }

  const round = gameState.round;
  if (
    !round ||
    !round.prompt_candidates ||
    round.prompt_candidates.length === 0
  ) {
    grid.innerHTML =
      '<p class="help-text" style="grid-column: 1 / -1; text-align: center;">Waiting for prompts...</p>';
    return;
  }

  const selectedId = round.selected_prompt ? round.selected_prompt.id : null;

  grid.innerHTML = round.prompt_candidates
    .map((prompt, idx) => {
      const isSelected = prompt.id === selectedId;
      return `
                <div class="prompt-card ${isSelected ? "selected" : ""}">
                    <div class="number">Prompt ${String.fromCharCode(65 + idx)}</div>
                    <div class="text">${escapeHtml(prompt.text || "Pending prompt...")}</div>
                </div>
            `;
    })
    .join("");
}

function updateWritingScene() {
  const promptEl = document.getElementById("writingPrompt");
  if (gameState.round?.selected_prompt) {
    promptEl.textContent =
      gameState.round.selected_prompt.text || "Loading prompt...";
  }
  // TODO: implement actual timer countdown
}

function updateRevealScene() {
  const numberEl = document.getElementById("revealNumber");
  const textEl = document.getElementById("revealText");

  if (!numberEl || !textEl) return;

  if (gameState.submissions.length > 0) {
    numberEl.textContent = "Answer 1";
    textEl.textContent = gameState.submissions[0].display_text;
  } else {
    numberEl.textContent = "Awaiting answers";
    textEl.textContent =
      "Submissions will appear here once the host begins the reveal.";
  }
}

function updateSubmissions() {
  if (currentPhase === "REVEAL") {
    updateRevealScene();
  }
  if (currentPhase === "VOTING") {
    initializeVoteBars();
    updateVoteBars();
  }
}

function initializeVoteBars() {
  const aiContainer = document.getElementById("aiVoteBars");
  const funnyContainer = document.getElementById("funnyVoteBars");

  aiContainer.innerHTML = "";
  funnyContainer.innerHTML = "";

  if (gameState.submissions.length === 0) {
    const placeholder =
      '<p class="help-text" style="text-align: center; width: 100%;">Waiting for submissions...</p>';
    aiContainer.innerHTML = placeholder;
    funnyContainer.innerHTML = placeholder;
    return;
  }

  gameState.submissions.forEach((sub, idx) => {
    const label = `Answer ${idx + 1}`;
    aiContainer.innerHTML += createVoteBar(sub.id, label, "ai");
    funnyContainer.innerHTML += createVoteBar(sub.id, label, "funny");
  });
}

function createVoteBar(subId, label, category) {
  return `
        <div class="vote-bar">
            <div class="vote-bar-label">${label}</div>
            <div class="vote-bar-track">
                <div class="vote-bar-fill" id="bar-${category}-${subId}" style="width: 0%">
                    <span class="vote-bar-count" id="count-${category}-${subId}">0</span>
                </div>
            </div>
        </div>
    `;
}

function updateVoteBars() {
  // Calculate max votes for scaling
  const allVotes = [
    ...Object.values(gameState.voteCounts.ai),
    ...Object.values(gameState.voteCounts.funny),
  ];
  const maxVotes = Math.max(...allVotes, 1);

  // Update AI bars
  Object.entries(gameState.voteCounts.ai).forEach(([subId, count]) => {
    const bar = document.getElementById(`bar-ai-${subId}`);
    const countEl = document.getElementById(`count-ai-${subId}`);
    if (bar && countEl) {
      const percentage = (count / maxVotes) * 100;
      bar.style.width = `${Math.max(percentage, 10)}%`;
      countEl.textContent = count;
    }
  });

  Object.entries(gameState.voteCounts.funny).forEach(([subId, count]) => {
    const bar = document.getElementById(`bar-funny-${subId}`);
    const countEl = document.getElementById(`count-funny-${subId}`);
    if (bar && countEl) {
      const percentage = (count / maxVotes) * 100;
      bar.style.width = `${Math.max(percentage, 10)}%`;
      countEl.textContent = count;
    }
  });
}

function findSubmissionById(subId) {
  return gameState.submissions.find((s) => s.id === subId);
}

function updateResults() {
  const aiRevealEl = document.getElementById("aiReveal");
  if (aiRevealEl) {
    const aiId = gameState.round ? gameState.round.ai_submission_id : null;
    if (aiId) {
      const aiSub = findSubmissionById(aiId);
      aiRevealEl.textContent = aiSub
        ? aiSub.display_text
        : "AI answer will be revealed shortly…";
    } else {
      aiRevealEl.textContent = "AI answer will be revealed shortly…";
    }
  }

  // Find funniest (most funny votes)
  const funnyVotes = gameState.voteCounts.funny;
  let funniestId = null;
  let maxFunny = 0;
  Object.entries(funnyVotes).forEach(([subId, count]) => {
    if (count > maxFunny) {
      maxFunny = count;
      funniestId = subId;
    }
  });

  const funniestEl = document.getElementById("funniestReveal");
  if (funniestEl) {
    if (funniestId) {
      const funniestSub = findSubmissionById(funniestId);
      funniestEl.textContent = funniestSub
        ? funniestSub.display_text
        : "Funniest answer incoming…";
    } else {
      funniestEl.textContent = "Funniest answer incoming…";
    }
  }

  // Update leaderboard
  const container = document.getElementById("leaderboardEntries");
  container.innerHTML = "";

  gameState.scores.players.forEach((score, idx) => {
    const isTop3 = idx < 3;
    container.innerHTML += `
            <div class="leaderboard-entry ${isTop3 ? "top-3" : ""}">
                <span>${idx + 1}. Player ${score.ref_id.substring(0, 8)}</span>
                <span>${score.total} pts</span>
            </div>
        `;
  });
}

function updatePodium() {
  const players = gameState.scores.players.slice(0, 3);

  if (players[0]) {
    document.getElementById("podium1Name").textContent =
      `Player ${players[0].ref_id.substring(0, 8)}`;
    document.getElementById("podium1Score").textContent =
      `${players[0].total} pts`;
  }

  if (players[1]) {
    document.getElementById("podium2Name").textContent =
      `Player ${players[1].ref_id.substring(0, 8)}`;
    document.getElementById("podium2Score").textContent =
      `${players[1].total} pts`;
  }

  if (players[2]) {
    document.getElementById("podium3Name").textContent =
      `Player ${players[2].ref_id.substring(0, 8)}`;
    document.getElementById("podium3Score").textContent =
      `${players[2].total} pts`;
  }
}

// Start the app
init();
