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
  revealIndex: 0,
  currentRevealSubmission: null, // Current submission being shown in reveal
};

// Timer and TTS managers
let writingTimer = null;
let votingTimer = null;
let ttsManager = null;

// Initialize on page load
function init() {
  wsConn = new WSConnection("beamer", handleMessage, updateConnectionStatus);
  wsConn.connect();

  // Initialize timers and TTS
  writingTimer = new CountdownTimer("writingTimer");
  votingTimer = new CountdownTimer("votingTimer");
  ttsManager = new TTSManager();
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      console.log("Welcome message:", message);
      if (message.game) {
        // Pass deadline and server_now for timer synchronization
        updatePhase(
          message.game.phase,
          message.game.phase_deadline,
          message.server_now,
        );
      }
      break;

    case "phase":
      updatePhase(message.phase, message.deadline, message.server_now);
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

    case "reveal_update":
      gameState.revealIndex = message.reveal_index;
      gameState.currentRevealSubmission = message.submission;
      if (message.submission) {
        updateRevealWithSubmission(message.submission, message.reveal_index);
        // Auto-read with TTS
        ttsManager.speak(message.submission.display_text, { rate: 0.9 });
      }
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

function updatePhase(phase, deadline, serverNow) {
  console.log(
    "Phase transition:",
    currentPhase,
    "->",
    phase,
    "deadline:",
    deadline,
  );

  const phaseChanged = currentPhase !== phase;
  currentPhase = phase;

  if (phaseChanged) {
    // Clear reveal state when leaving REVEAL phase
    if (phase !== "REVEAL") {
      gameState.currentRevealSubmission = null;
    }

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

  // Start timer for timed phases
  if (phase === "WRITING") {
    writingTimer.start(deadline, serverNow);
    votingTimer.stop();
  } else if (phase === "VOTING") {
    votingTimer.start(deadline, serverNow);
    writingTimer.stop();
  } else {
    writingTimer.stop();
    votingTimer.stop();
  }
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
      '<p class="help-text" style="grid-column: 1 / -1; text-align: center;">Warte auf Fragen...</p>';
    return;
  }

  const selectedId = round.selected_prompt ? round.selected_prompt.id : null;

  grid.innerHTML = round.prompt_candidates
    .map((prompt, idx) => {
      const isSelected = prompt.id === selectedId;
      return `
                <div class="prompt-card ${isSelected ? "selected" : ""}">
                    <div class="number">Frage ${String.fromCharCode(65 + idx)}</div>
                    <div class="text">${escapeHtml(prompt.text || "Frage wird vorbereitet...")}</div>
                </div>
            `;
    })
    .join("");
}

function updateWritingScene() {
  const promptEl = document.getElementById("writingPrompt");
  if (gameState.round?.selected_prompt) {
    promptEl.textContent =
      gameState.round.selected_prompt.text || "Frage wird geladen...";
  }
}

function updateRevealScene() {
  const numberEl = document.getElementById("revealNumber");
  const textEl = document.getElementById("revealText");

  if (!numberEl || !textEl) return;

  // Only show submission from reveal_update messages, not from arbitrary submissions array
  if (gameState.currentRevealSubmission) {
    numberEl.textContent = `Antwort ${gameState.revealIndex + 1}`;
    textEl.textContent = gameState.currentRevealSubmission.display_text;
  } else {
    numberEl.textContent = "Warte auf Antworten";
    textEl.textContent =
      "Die Antworten erscheinen hier, sobald der Host die Präsentation startet.";
  }
}

function updateRevealWithSubmission(submission, index) {
  const numberEl = document.getElementById("revealNumber");
  const textEl = document.getElementById("revealText");

  if (!numberEl || !textEl) return;

  numberEl.textContent = `Antwort ${index + 1}`;
  textEl.textContent = submission.display_text;

  // Add fade-in animation
  const card = document.querySelector("#scene-REVEAL .answer-card");
  if (card) {
    card.classList.remove("fadeIn");
    // Trigger reflow to restart animation
    void card.offsetWidth;
    card.classList.add("fadeIn");
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
      '<p class="help-text" style="text-align: center; width: 100%;">Warte auf Antworten...</p>';
    aiContainer.innerHTML = placeholder;
    funnyContainer.innerHTML = placeholder;
    return;
  }

  gameState.submissions.forEach((sub, idx) => {
    const label = `Antwort ${idx + 1}`;
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
        : "KI-Antwort wird gleich enthüllt…";
    } else {
      aiRevealEl.textContent = "KI-Antwort wird gleich enthüllt…";
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
        : "Lustigste Antwort kommt gleich…";
    } else {
      funniestEl.textContent = "Lustigste Antwort kommt gleich…";
    }
  }

  // Update leaderboard
  const container = document.getElementById("leaderboardEntries");
  container.innerHTML = "";

  gameState.scores.players.forEach((score, idx) => {
    const isTop3 = idx < 3;
    container.innerHTML += `
            <div class="leaderboard-entry ${isTop3 ? "top-3" : ""}">
                <span>${idx + 1}. Spieler ${score.ref_id.substring(0, 8)}</span>
                <span>${score.total} Pkt</span>
            </div>
        `;
  });
}

function updatePodium() {
  const players = gameState.scores.players.slice(0, 3);

  if (players[0]) {
    document.getElementById("podium1Name").textContent =
      `Spieler ${players[0].ref_id.substring(0, 8)}`;
    document.getElementById("podium1Score").textContent =
      `${players[0].total} Pkt`;
  }

  if (players[1]) {
    document.getElementById("podium2Name").textContent =
      `Spieler ${players[1].ref_id.substring(0, 8)}`;
    document.getElementById("podium2Score").textContent =
      `${players[1].total} Pkt`;
  }

  if (players[2]) {
    document.getElementById("podium3Name").textContent =
      `Spieler ${players[2].ref_id.substring(0, 8)}`;
    document.getElementById("podium3Score").textContent =
      `${players[2].total} Pkt`;
  }
}

// Start the app
init();
