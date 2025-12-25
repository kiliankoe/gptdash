/**
 * GPTDash Beamer Display - 39C3 Edition
 * Full-screen stage display for projector/TV
 */

import {
  WSConnection,
  CountdownTimer,
  QRCodeManager,
  escapeHtml,
  updateConnectionStatus,
  renderPromptDisplay,
} from "./common.js";

// Game state
const gameState = {
  phase: "LOBBY",
  roundNo: 0,
  currentRound: null,
  submissions: [],
  submissionCount: 0, // Used when texts are intentionally not broadcast (anti-spoiler)
  revealIndex: 0,
  currentRevealSubmission: null,
  scores: { players: [], audienceTop: [] },
  voteCounts: { ai: {}, funny: {} },
  promptCandidates: [], // Prompts for voting during PROMPT_SELECTION
  promptVoteCounts: {}, // Vote counts per prompt during PROMPT_SELECTION
  // Trivia state
  activeTrivia: null, // Current trivia question being shown
  triviaResult: null, // Trivia result after resolve
  // Manual winners (panic mode)
  manualAiWinner: null,
  manualFunnyWinner: null,
};

// Connections and utilities
let ws = null;
let timer = null;

const MAX_PLAYER_LEADERBOARD_ROWS = 6;
const MAX_AUDIENCE_LEADERBOARD_ROWS = 6;

// Initialize on page load
document.addEventListener("DOMContentLoaded", () => {
  initializeBeamer();
});

function initializeBeamer() {
  // Initialize timer (using phase-specific timers, no footer timer)
  timer = new CountdownTimer("writingTimer", onTimerComplete);

  // Generate QR codes and set URL
  generateQRCodes();

  // Connect to WebSocket
  connectWebSocket();
}

function generateQRCodes() {
  const url = QRCodeManager.getAudienceJoinUrl();

  // Lobby QR (large)
  QRCodeManager.generate("lobbyQR", url, {
    width: 250,
    height: 250,
    colorDark: "#141414",
    colorLight: "#faf5f5",
  });

  // Set URL text in header and lobby
  const displayUrl = url.replace(/^https?:\/\//, "").replace(/\/$/, "");
  const lobbyUrlEl = document.getElementById("lobbyUrl");
  const headerUrlEl = document.getElementById("headerUrl");
  if (lobbyUrlEl) lobbyUrlEl.textContent = displayUrl;
  if (headerUrlEl) headerUrlEl.textContent = displayUrl;
}

function connectWebSocket() {
  ws = new WSConnection("beamer", handleMessage, updateConnectionStatus);
  ws.connect();
}

function handleMessage(msg) {
  console.log("[Beamer] Received:", msg.t, msg);

  switch (msg.t) {
    case "welcome":
      handleWelcome(msg);
      break;
    case "phase":
      handlePhaseChange(msg);
      break;
    case "round_started":
      handleRoundStarted(msg);
      break;
    case "prompt_selected":
      handlePromptSelected(msg);
      break;
    case "submissions":
      handleSubmissions(msg);
      break;
    case "submission_count":
      handleSubmissionCount(msg);
      break;
    case "reveal_update":
      handleRevealUpdate(msg);
      break;
    case "beamer_vote_counts":
      handleVoteCounts(msg);
      break;
    case "scores":
      handleScores(msg);
      break;
    case "game_state":
      handleGameState(msg);
      break;
    case "deadline_update":
      handleDeadlineUpdate(msg);
      break;
    case "prompt_candidates":
      handlePromptCandidates(msg);
      break;
    case "beamer_prompt_vote_counts":
      handlePromptVoteCounts(msg);
      break;
    // Trivia messages
    case "trivia_question":
      handleTriviaQuestion(msg);
      break;
    case "trivia_result":
      handleTriviaResult(msg);
      break;
    case "trivia_clear":
      handleTriviaClear();
      break;
    // Manual winners (panic mode)
    case "manual_winners":
      handleManualWinners(msg);
      break;
    // Vote label reveal (host action)
    case "vote_labels_revealed":
      handleVoteLabelsRevealed();
      break;
    case "error":
      console.error("[Beamer] Error:", msg.code, msg.msg);
      break;
  }
}

// ========================
// Message Handlers
// ========================

function handleWelcome(msg) {
  console.log("[Beamer] Welcome! Role:", msg.role);
  if (msg.game) {
    gameState.phase = msg.game.phase;
    gameState.roundNo = msg.game.round_no;
    updateRoundBadge();
    showScene(phaseToScene(msg.game.phase));

    // Start timer if deadline exists
    if (msg.game.phase_deadline) {
      timer.start(msg.game.phase_deadline, msg.server_now);
    }
  }
}

function handlePhaseChange(msg) {
  const previousPhase = gameState.phase;
  // Detect round changes via phase broadcast (covers implicit round starts)
  if (typeof msg.round_no === "number" && msg.round_no !== gameState.roundNo) {
    resetRoundUiState();
  }
  gameState.phase = msg.phase;
  gameState.roundNo = msg.round_no;
  updateRoundBadge();

  // Switch timer element based on phase
  if (msg.phase === "WRITING") {
    timer.setElement("writingTimer");
  } else if (msg.phase === "VOTING") {
    timer.setElement("votingTimer");
  }

  // Handle timer
  if (msg.deadline) {
    timer.start(msg.deadline, msg.server_now);
  } else {
    timer.stop();
    timer.hide();
  }

  // Update prompt if included in phase message (sent during WRITING transition)
  if (msg.prompt) {
    if (!gameState.currentRound) {
      gameState.currentRound = { selected_prompt: msg.prompt };
    } else {
      gameState.currentRound.selected_prompt = msg.prompt;
    }
  }

  // Clear reveal state when leaving REVEAL phase
  if (previousPhase === "REVEAL" && msg.phase !== "REVEAL") {
    gameState.currentRevealSubmission = null;
  }

  // Clear trivia when leaving WRITING phase
  if (previousPhase === "WRITING" && msg.phase !== "WRITING") {
    gameState.activeTrivia = null;
    gameState.triviaResult = null;
    hideTriviaOverlays();
  }

  showScene(phaseToScene(msg.phase));
}

function handleRoundStarted(msg) {
  resetRoundUiState();
  gameState.currentRound = msg.round;
  gameState.roundNo = msg.round.number;
  updateRoundBadge();

  // Update prompt candidates for selection
  if (msg.round.prompt_candidates && msg.round.prompt_candidates.length > 0) {
    updatePromptCandidates(msg.round.prompt_candidates);
  }
}

function resetRoundUiState() {
  gameState.currentRound = null;
  gameState.submissions = [];
  gameState.submissionCount = 0;
  gameState.revealIndex = 0;
  gameState.currentRevealSubmission = null;
  gameState.voteCounts = { ai: {}, funny: {} };
  gameState.promptCandidates = [];
  gameState.promptVoteCounts = {};
  gameState.manualAiWinner = null;
  gameState.manualFunnyWinner = null;
  updateSubmissionCounter();
  updateRevealIndicator();
  clearPromptDisplay();
}

function clearPromptDisplay() {
  const promptText = document.getElementById("writingPromptText");
  const promptImage = document.getElementById("writingPromptImage");

  if (promptText) {
    promptText.textContent = "";
    promptText.style.display = "none";
  }
  if (promptImage) {
    promptImage.innerHTML = "";
    promptImage.style.display = "none";
  }
}

function handlePromptSelected(msg) {
  if (msg.prompt) {
    // Initialize currentRound if it doesn't exist (can happen if round was
    // auto-created during prompt selection and RoundStarted wasn't broadcast)
    if (!gameState.currentRound) {
      gameState.currentRound = {
        selected_prompt: msg.prompt,
      };
    } else {
      gameState.currentRound.selected_prompt = msg.prompt;
    }

    // Update display with text and/or image
    updatePromptDisplay(msg.prompt);
  }
}

/**
 * Update the prompt display with text and/or image
 */
function updatePromptDisplay(prompt) {
  const promptText = document.getElementById("writingPromptText");
  const promptImage = document.getElementById("writingPromptImage");
  renderPromptDisplay(prompt, promptText, promptImage);
}

function handleSubmissions(msg) {
  gameState.submissions = msg.list || [];
  gameState.submissionCount = gameState.submissions.length;
  updateSubmissionCounter();

  // Reinitialize vote bars if we're in voting phase
  if (gameState.phase === "VOTING") {
    initVotingBars();
    updateVoteBarsAnimated();
  }

  // Update result reveals if we're in results phase
  if (gameState.phase === "RESULTS") {
    updateResultReveals();
  }

  updateRevealIndicator();
}

function handleSubmissionCount(msg) {
  gameState.submissionCount = msg.count || 0;
  updateSubmissionCounter();
  updateRevealIndicator();
}

function handleRevealUpdate(msg) {
  gameState.revealIndex = msg.reveal_index;
  gameState.currentRevealSubmission = msg.submission;

  if (msg.submission) {
    showRevealCard(msg.submission, msg.reveal_index);
  }

  updateRevealIndicator();
}

function handleVoteCounts(msg) {
  gameState.voteCounts = {
    ai: msg.ai || {},
    funny: msg.funny || {},
  };
  updateVoteBarsAnimated();

  // Update result reveals if we're in results phase
  if (gameState.phase === "RESULTS") {
    updateResultReveals();
  }
}

function handleVoteLabelsRevealed() {
  document.querySelectorAll(".vote-bar-label").forEach((label) => {
    label.classList.remove("hidden");
  });
}

function handlePromptCandidates(msg) {
  gameState.promptCandidates = msg.prompts || [];
  gameState.promptVoteCounts = {}; // Reset vote counts
  updatePromptSelectionScene();
}

function handlePromptVoteCounts(msg) {
  gameState.promptVoteCounts = msg.counts || {};
  updatePromptVoteBars();
}

function handleScores(msg) {
  gameState.scores = {
    players: msg.players || [],
    audienceTop: msg.audience_top || [],
  };

  // Store ai_submission_id for result label logic
  if (msg.ai_submission_id) {
    if (!gameState.currentRound) {
      gameState.currentRound = {};
    }
    gameState.currentRound.ai_submission_id = msg.ai_submission_id;
  }

  updateLeaderboard();
  updatePodium();
  updateResultReveals();
}

function handleGameState(msg) {
  if (msg.game) {
    gameState.phase = msg.game.phase;
    gameState.roundNo = msg.game.round_no;
    updateRoundBadge();
    showScene(phaseToScene(msg.game.phase));
  }
}

function handleDeadlineUpdate(msg) {
  // Update timer when deadline is extended
  if (timer && msg.deadline && msg.server_now) {
    timer.updateDeadline(msg.deadline, msg.server_now);
    console.log("[Beamer] Timer updated with new deadline:", msg.deadline);
  }
}

// ========================
// Scene Management
// ========================

function phaseToScene(phase) {
  const mapping = {
    LOBBY: "sceneLobby",
    PROMPT_SELECTION: "scenePromptSelection",
    WRITING: "sceneWriting",
    REVEAL: "sceneReveal",
    VOTING: "sceneVoting",
    RESULTS: "sceneResults",
    PODIUM: "scenePodium",
    INTERMISSION: "sceneIntermission",
    ENDED: "sceneEnded",
  };
  return mapping[phase] || "sceneLobby";
}

function showScene(sceneId) {
  // Hide all scenes
  document.querySelectorAll(".scene").forEach((scene) => {
    scene.classList.remove("active");
  });

  // Show target scene
  const scene = document.getElementById(sceneId);
  if (scene) {
    scene.classList.add("active");
  }

  // Phase-specific initialization
  if (sceneId === "sceneVoting") {
    initVotingBars();
  } else if (sceneId === "sceneWriting") {
    updateWritingScene();
  } else if (sceneId === "sceneReveal") {
    updateRevealScene();
  } else if (sceneId === "sceneResults") {
    updateResultReveals();
  } else if (sceneId === "scenePodium") {
    triggerPodiumConfetti();
  }
}

function triggerPodiumConfetti() {
  if (typeof window.confetti !== "function") return;

  const w = window.innerWidth;
  const h = window.innerHeight;

  // Fire confetti from multiple positions with staggered timing
  window.confetti({
    position: { x: w * 0.2, y: h * 0.5 },
    count: 100,
    velocity: 250,
  });
  window.confetti({
    position: { x: w * 0.8, y: h * 0.5 },
    count: 100,
    velocity: 250,
  });

  setTimeout(() => {
    window.confetti({
      position: { x: w * 0.5, y: h * 0.3 },
      count: 150,
      velocity: 300,
    });
  }, 300);

  setTimeout(() => {
    window.confetti({
      position: { x: w * 0.3, y: h * 0.4 },
      count: 75,
      velocity: 200,
    });
    window.confetti({
      position: { x: w * 0.7, y: h * 0.4 },
      count: 75,
      velocity: 200,
    });
  }, 600);
}

// ========================
// UI Updates
// ========================

function updateRoundBadge() {
  const badge = document.getElementById("roundBadge");
  if (badge) {
    badge.textContent = `Runde ${gameState.roundNo}`;
  }
}

function updatePromptCandidates(candidates) {
  candidates.forEach((prompt, index) => {
    const textEl = document.getElementById(`promptText${index}`);
    if (textEl) {
      textEl.textContent = prompt.text || "(Bildfrage)";
    }

    const card = document.querySelector(`[data-prompt-index="${index}"]`);
    if (card) {
      card.classList.remove("selected", "highlighted");
      card.style.display = "flex";
    }
  });

  // Hide empty slots
  for (let i = candidates.length; i < 3; i++) {
    const card = document.querySelector(`[data-prompt-index="${i}"]`);
    if (card) {
      card.style.display = "none";
    }
  }
}

function updateWritingScene() {
  if (gameState.currentRound?.selected_prompt) {
    updatePromptDisplay(gameState.currentRound.selected_prompt);
  }
}

function updateSubmissionCounter() {
  const counter = document.getElementById("submissionCounter");
  if (counter) {
    const count =
      gameState.submissions.length || gameState.submissionCount || 0;
    if (count === 0) {
      counter.textContent = "Antworten werden gesammelt…";
      return;
    }
    counter.textContent =
      count === 1 ? "1 Antwort eingereicht" : `${count} Antworten eingereicht`;
  }
}

function initVotingBars() {
  // Display the current prompt at the top of the voting scene
  const votingPromptText = document.getElementById("votingPromptText");
  const votingPromptImage = document.getElementById("votingPromptImage");
  if (gameState.currentRound?.selected_prompt) {
    renderPromptDisplay(
      gameState.currentRound.selected_prompt,
      votingPromptText,
      votingPromptImage,
    );
  }

  const submissions = gameState.submissions;
  if (submissions.length === 0) return;

  const aiContainer = document.getElementById("aiVoteBars");
  const funnyContainer = document.getElementById("funnyVoteBars");

  if (!aiContainer || !funnyContainer) return;

  aiContainer.innerHTML = "";
  funnyContainer.innerHTML = "";

  // Shuffle submissions for anonymous display (same order for both categories)
  const shuffled = [...submissions].sort(() => Math.random() - 0.5);

  shuffled.forEach((sub) => {
    // AI vote bar
    aiContainer.appendChild(createVoteBar(sub.id, sub.display_text, "ai"));
    // Funny vote bar
    funnyContainer.appendChild(
      createVoteBar(sub.id, sub.display_text, "funny"),
    );
  });
}

function createVoteBar(id, text, type) {
  const bar = document.createElement("div");
  bar.className = "vote-bar";
  bar.dataset.submissionId = id;
  bar.innerHTML = `
        <div class="vote-bar-label hidden">${escapeHtml(text)}</div>
        <div class="vote-bar-track">
            <div class="vote-bar-fill ${type}" style="width: 0%"></div>
            <div class="vote-bar-count">0</div>
        </div>
    `;
  return bar;
}

function updateVoteBarsAnimated() {
  const aiCounts = gameState.voteCounts.ai;
  const funnyCounts = gameState.voteCounts.funny;

  // Calculate totals for percentage
  const aiTotal = Object.values(aiCounts).reduce((a, b) => a + b, 0) || 1;
  const funnyTotal = Object.values(funnyCounts).reduce((a, b) => a + b, 0) || 1;

  // Update AI bars
  document.querySelectorAll("#aiVoteBars .vote-bar").forEach((bar) => {
    const id = bar.dataset.submissionId;
    const count = aiCounts[id] || 0;
    const percent = Math.min((count / aiTotal) * 100, 100);
    const fill = bar.querySelector(".vote-bar-fill");
    const countEl = bar.querySelector(".vote-bar-count");
    if (fill) fill.style.width = `${percent}%`;
    if (countEl) countEl.textContent = count;
  });

  // Update Funny bars
  document.querySelectorAll("#funnyVoteBars .vote-bar").forEach((bar) => {
    const id = bar.dataset.submissionId;
    const count = funnyCounts[id] || 0;
    const percent = Math.min((count / funnyTotal) * 100, 100);
    const fill = bar.querySelector(".vote-bar-fill");
    const countEl = bar.querySelector(".vote-bar-count");
    if (fill) fill.style.width = `${percent}%`;
    if (countEl) countEl.textContent = count;
  });
}

function updatePromptSelectionScene() {
  const grid = document.getElementById("promptGrid");
  if (!grid) return;

  const prompts = gameState.promptCandidates;

  if (prompts.length === 0) {
    grid.innerHTML =
      '<div class="prompt-card"><div class="prompt-text">Warte auf Prompts...</div></div>';
    return;
  }

  // Calculate max votes for percentage
  const counts = gameState.promptVoteCounts;
  const maxVotes =
    Math.max(...Object.values(counts), 1) || prompts.length > 0 ? 1 : 0;

  grid.innerHTML = prompts
    .map((prompt, idx) => {
      const voteCount = counts[prompt.id] || 0;
      const percent = maxVotes > 0 ? (voteCount / maxVotes) * 100 : 0;

      let contentHtml = "";
      if (prompt.image_url) {
        contentHtml += `<div class="prompt-image"><img src="${escapeHtml(prompt.image_url)}" alt="Prompt Bild" style="max-height: 150px; border-radius: 8px;"></div>`;
      }
      if (prompt.text) {
        contentHtml += `<div class="prompt-text">${escapeHtml(prompt.text)}</div>`;
      }

      return `
        <div class="prompt-card" data-prompt-id="${prompt.id}">
          <div class="prompt-number">${idx + 1}</div>
          ${contentHtml}
          <div class="prompt-vote-bar">
            <div class="prompt-vote-bar-fill" style="width: ${percent}%"></div>
            <span class="prompt-vote-count">${voteCount}</span>
          </div>
        </div>
      `;
    })
    .join("");
}

function updatePromptVoteBars() {
  const counts = gameState.promptVoteCounts;
  const maxVotes = Math.max(...Object.values(counts), 1) || 1;

  document.querySelectorAll("#promptGrid .prompt-card").forEach((card) => {
    const id = card.dataset.promptId;
    const count = counts[id] || 0;
    const percent = (count / maxVotes) * 100;

    const fill = card.querySelector(".prompt-vote-bar-fill");
    const countEl = card.querySelector(".prompt-vote-count");

    if (fill) fill.style.width = `${percent}%`;
    if (countEl) countEl.textContent = count;
  });
}

function updateRevealIndicator() {
  const container = document.getElementById("revealIndicator");
  if (!container) return;

  const count = gameState.submissions.length || gameState.submissionCount || 0;
  const current = gameState.revealIndex;

  container.innerHTML = "";
  for (let i = 0; i < count; i++) {
    const dot = document.createElement("div");
    dot.className = "reveal-dot";
    if (i < current) {
      dot.classList.add("revealed");
    } else if (i === current) {
      dot.classList.add("active");
    }
    container.appendChild(dot);
  }
}

function updateRevealScene() {
  const numberEl = document.getElementById("revealNumber");
  const textEl = document.getElementById("revealText");

  if (!numberEl || !textEl) return;

  if (gameState.currentRevealSubmission) {
    const total =
      gameState.submissions.length || gameState.submissionCount || 0;
    numberEl.textContent =
      total > 0
        ? `Antwort ${gameState.revealIndex + 1} von ${total}`
        : `Antwort ${gameState.revealIndex + 1}`;
    textEl.textContent = gameState.currentRevealSubmission.display_text;
  } else {
    numberEl.textContent = "Warte auf Antworten";
    textEl.textContent =
      "Die Antworten erscheinen hier, sobald der Host die Praesentation startet.";
  }
}

function showRevealCard(submission, index) {
  const total = gameState.submissions.length || gameState.submissionCount || 0;
  const numberEl = document.getElementById("revealNumber");
  const textEl = document.getElementById("revealText");

  if (numberEl) {
    numberEl.textContent =
      total > 0 ? `Antwort ${index + 1} von ${total}` : `Antwort ${index + 1}`;
  }
  if (textEl) textEl.textContent = submission.display_text;

  // Animate card
  const card = document.getElementById("revealCard");
  if (card) {
    card.style.animation = "none";
    card.offsetHeight; // Trigger reflow
    card.style.animation = "slideUp 0.6s ease-out";
  }

  updateRevealIndicator();
}

function updateLeaderboard() {
  const container = document.getElementById("leaderboardList");
  if (!container) return;

  const players = gameState.scores.players;

  if (players.length === 0) {
    container.innerHTML =
      '<div class="body-text" style="opacity: 0.6; text-align: center;">Noch keine Punkte</div>';
    return;
  }

  // Sort by total score descending
  const sorted = [...players].sort((a, b) => b.total - a.total);

  const rows = sorted
    .slice(0, MAX_PLAYER_LEADERBOARD_ROWS)
    .map((player, index) => {
      const rankClass =
        index === 0
          ? "gold"
          : index === 1
            ? "silver"
            : index === 2
              ? "bronze"
              : "";
      const displayName = player.display_name || `Spieler ${index + 1}`;
      return `
            <div class="leaderboard-row" style="animation-delay: ${index * 0.1}s">
                <div class="leaderboard-rank ${rankClass}">${index + 1}</div>
                <div class="leaderboard-name">${escapeHtml(displayName)}</div>
                <div class="leaderboard-score">${player.total}</div>
            </div>
        `;
    })
    .join("");

  const remaining = sorted.length - MAX_PLAYER_LEADERBOARD_ROWS;
  const more =
    remaining > 0
      ? `<div class="leaderboard-more">+${remaining} weitere</div>`
      : "";

  container.innerHTML = rows + more;

  // Also update audience leaderboard
  updateAudienceLeaderboard();
}

function updateAudienceLeaderboard() {
  const container = document.getElementById("audienceLeaderboardList");
  if (!container) return;

  const audience = gameState.scores.audienceTop;

  if (!audience || audience.length === 0) {
    container.innerHTML =
      '<div class="body-text" style="opacity: 0.6; text-align: center;">Noch keine Detektive</div>';
    return;
  }

  // Sort by total score descending (should already be sorted, but ensure)
  const sorted = [...audience].sort((a, b) => b.total - a.total);

  container.innerHTML = sorted
    .slice(0, MAX_AUDIENCE_LEADERBOARD_ROWS)
    .map((member, index) => {
      const rankClass =
        index === 0
          ? "gold"
          : index === 1
            ? "silver"
            : index === 2
              ? "bronze"
              : "";
      // Use display_name from server (auto-generated friendly name) or fallback
      const displayName = member.display_name || `Detektiv ${index + 1}`;
      return `
            <div class="leaderboard-row" style="animation-delay: ${index * 0.1}s">
                <div class="leaderboard-rank ${rankClass}">${index + 1}</div>
                <div class="leaderboard-name">${escapeHtml(displayName)}</div>
                <div class="leaderboard-score">${member.total}</div>
            </div>
        `;
    })
    .join("");

  const remaining = sorted.length - MAX_AUDIENCE_LEADERBOARD_ROWS;
  if (remaining > 0) {
    container.innerHTML += `<div class="leaderboard-more">+${remaining} weitere</div>`;
  }
}

function updateResultReveals() {
  const aiCounts = gameState.voteCounts.ai;
  const funnyCounts = gameState.voteCounts.funny;
  const actualAiId = gameState.currentRound?.ai_submission_id;

  // AI Winner: Use manual winner > most voted > actual AI (fallback)
  let aiDisplayId = gameState.manualAiWinner;
  const aiIsManual = !!aiDisplayId;
  if (!aiDisplayId) {
    let maxAiVotes = 0;
    for (const [id, count] of Object.entries(aiCounts)) {
      if (count > maxAiVotes) {
        maxAiVotes = count;
        aiDisplayId = id;
      }
    }
  }
  if (!aiDisplayId) {
    aiDisplayId = actualAiId;
  }

  // Funny Winner: Use manual winner > most voted
  let funnyDisplayId = gameState.manualFunnyWinner;
  const funnyIsManual = !!funnyDisplayId;
  if (!funnyDisplayId) {
    let maxFunnyVotes = 0;
    for (const [id, count] of Object.entries(funnyCounts)) {
      if (count > maxFunnyVotes) {
        maxFunnyVotes = count;
        funnyDisplayId = id;
      }
    }
  }

  // Update AI reveal
  const aiRevealAnswer = document.getElementById("aiRevealAnswer");
  const aiRevealMeta = document.getElementById("aiRevealMeta");
  const aiRevealLabel = document.getElementById("aiRevealLabel");

  // Update AI label based on ID comparison (independent of submission details)
  if (aiDisplayId && aiRevealLabel) {
    if (aiDisplayId === actualAiId) {
      aiRevealLabel.textContent = "KI richtig erkannt!";
    } else {
      aiRevealLabel.textContent = "Am erfolgreichsten als KI überzeugt";
    }
  }

  // Update AI answer and meta if submission found
  if (aiDisplayId && aiRevealAnswer) {
    const aiSub = gameState.submissions.find((s) => s.id === aiDisplayId);
    if (aiSub) {
      aiRevealAnswer.textContent = aiSub.display_text;
      if (aiRevealMeta) {
        const votes = aiCounts[aiDisplayId] || 0;
        aiRevealMeta.textContent = aiIsManual
          ? "(Host-Auswahl)"
          : `${votes} Stimmen`;
      }
    }
  }

  // Update Funny reveal
  const funnyRevealAnswer = document.getElementById("funnyRevealAnswer");
  const funnyRevealMeta = document.getElementById("funnyRevealMeta");
  const funnyRevealLabel = document.getElementById("funnyRevealLabel");

  // Update Funny label based on ID comparison (independent of submission details)
  if (funnyDisplayId && funnyRevealLabel) {
    if (funnyDisplayId === actualAiId) {
      funnyRevealLabel.textContent = "Die KI war am lustigsten?!";
    } else {
      funnyRevealLabel.textContent = "Am lustigsten";
    }
  }

  // Update Funny answer and meta if submission found
  if (funnyDisplayId && funnyRevealAnswer) {
    const funnySub = gameState.submissions.find((s) => s.id === funnyDisplayId);
    if (funnySub) {
      funnyRevealAnswer.textContent = funnySub.display_text;
      if (funnyRevealMeta) {
        const votes = funnyCounts[funnyDisplayId] || 0;
        funnyRevealMeta.textContent = funnyIsManual
          ? "(Host-Auswahl)"
          : `${votes} Stimmen`;
      }
    }
  }
}

function updatePodium() {
  // Update player podium
  updatePlayerPodium();
  // Update audience podium
  updateAudiencePodium();
}

function updatePlayerPodium() {
  const players = gameState.scores.players;
  if (players.length === 0) return;

  // Sort by total score descending
  const sorted = [...players].sort((a, b) => b.total - a.total);

  // First place
  if (sorted[0]) {
    const name1 = document.getElementById("podiumFirstName");
    const score1 = document.getElementById("podiumFirstScore");
    if (name1) name1.textContent = sorted[0].display_name || "Spieler 1";
    if (score1) score1.textContent = `${sorted[0].total} Punkte`;
  }

  // Second place
  const podiumSecond = document.getElementById("podiumSecond");
  if (sorted[1]) {
    const name2 = document.getElementById("podiumSecondName");
    const score2 = document.getElementById("podiumSecondScore");
    if (name2) name2.textContent = sorted[1].display_name || "Spieler 2";
    if (score2) score2.textContent = `${sorted[1].total} Punkte`;
    if (podiumSecond) podiumSecond.style.visibility = "visible";
  } else if (podiumSecond) {
    podiumSecond.style.visibility = "hidden";
  }

  // Third place
  const podiumThird = document.getElementById("podiumThird");
  if (sorted[2]) {
    const name3 = document.getElementById("podiumThirdName");
    const score3 = document.getElementById("podiumThirdScore");
    if (name3) name3.textContent = sorted[2].display_name || "Spieler 3";
    if (score3) score3.textContent = `${sorted[2].total} Punkte`;
    if (podiumThird) podiumThird.style.visibility = "visible";
  } else if (podiumThird) {
    podiumThird.style.visibility = "hidden";
  }
}

function updateAudiencePodium() {
  const audience = gameState.scores.audienceTop;

  // Hide entire audience section if no audience scores
  const audienceSection = document.querySelector(".audience-podium-section");
  if (!audience || audience.length === 0) {
    if (audienceSection) audienceSection.style.display = "none";
    return;
  }
  if (audienceSection) audienceSection.style.display = "flex";

  // Sort by total score descending (should already be sorted, but ensure)
  const sorted = [...audience].sort((a, b) => b.total - a.total);

  // First place
  const audienceFirst = document.getElementById("audiencePodiumFirst");
  if (sorted[0]) {
    const name1 = document.getElementById("audiencePodiumFirstName");
    const score1 = document.getElementById("audiencePodiumFirstScore");
    if (name1) name1.textContent = sorted[0].display_name || "Detektiv 1";
    if (score1) score1.textContent = `${sorted[0].total} Punkte`;
    if (audienceFirst) audienceFirst.style.visibility = "visible";
  } else if (audienceFirst) {
    audienceFirst.style.visibility = "hidden";
  }

  // Second place
  const audienceSecond = document.getElementById("audiencePodiumSecond");
  if (sorted[1]) {
    const name2 = document.getElementById("audiencePodiumSecondName");
    const score2 = document.getElementById("audiencePodiumSecondScore");
    if (name2) name2.textContent = sorted[1].display_name || "Detektiv 2";
    if (score2) score2.textContent = `${sorted[1].total} Punkte`;
    if (audienceSecond) audienceSecond.style.visibility = "visible";
  } else if (audienceSecond) {
    audienceSecond.style.visibility = "hidden";
  }

  // Third place
  const audienceThird = document.getElementById("audiencePodiumThird");
  if (sorted[2]) {
    const name3 = document.getElementById("audiencePodiumThirdName");
    const score3 = document.getElementById("audiencePodiumThirdScore");
    if (name3) name3.textContent = sorted[2].display_name || "Detektiv 3";
    if (score3) score3.textContent = `${sorted[2].total} Punkte`;
    if (audienceThird) audienceThird.style.visibility = "visible";
  } else if (audienceThird) {
    audienceThird.style.visibility = "hidden";
  }
}

function onTimerComplete() {
  // Timer finished - add warning class to timers
  const writingTimer = document.getElementById("writingTimer");
  const votingTimer = document.getElementById("votingTimer");

  [writingTimer, votingTimer].forEach((el) => {
    if (el) {
      el.classList.add("warning");
    }
  });
}

// ========================
// Manual Winners Handler (Panic Mode)
// ========================

function handleManualWinners(msg) {
  console.log("[Beamer] Manual winners received:", msg);
  gameState.manualAiWinner = msg.ai_winner_id;
  gameState.manualFunnyWinner = msg.funny_winner_id;
  if (gameState.phase === "RESULTS") {
    updateResultReveals();
  }
}

// ========================
// Trivia Handlers
// ========================

function handleTriviaQuestion(msg) {
  console.log("[Beamer] Trivia question received:", msg);
  gameState.activeTrivia = {
    question_id: msg.question_id,
    question: msg.question,
    image_url: msg.image_url,
    choices: msg.choices,
  };
  gameState.triviaResult = null; // Clear any previous result
  showTriviaOverlay();
}

function handleTriviaResult(msg) {
  console.log("[Beamer] Trivia result received:", msg);
  gameState.triviaResult = {
    question_id: msg.question_id,
    question: msg.question,
    image_url: msg.image_url,
    choices: msg.choices,
    correct_index: msg.correct_index,
    vote_counts: msg.vote_counts,
    total_votes: msg.total_votes,
  };
  gameState.activeTrivia = null; // Clear active question
  showTriviaResult();
}

function handleTriviaClear() {
  console.log("[Beamer] Trivia cleared");
  gameState.activeTrivia = null;
  gameState.triviaResult = null;
  hideTriviaOverlays();
}

// Dynamic labels for trivia choices (2-4)
const TRIVIA_LABELS = ["A", "B", "C", "D"];

function showTriviaOverlay() {
  const overlay = document.getElementById("triviaOverlay");
  const resultOverlay = document.getElementById("triviaResultOverlay");
  const questionEl = document.getElementById("triviaQuestion");
  const questionImageEl = document.getElementById("triviaQuestionImage");
  const choicesEl = document.getElementById("triviaChoices");

  if (!overlay || !questionEl || !choicesEl) return;

  // Hide result overlay if showing
  if (resultOverlay) resultOverlay.style.display = "none";

  // Update question
  questionEl.textContent = gameState.activeTrivia.question;

  // Update question image
  if (questionImageEl) {
    if (gameState.activeTrivia.image_url) {
      questionImageEl.src = gameState.activeTrivia.image_url;
      questionImageEl.style.display = "block";
    } else {
      questionImageEl.style.display = "none";
    }
  }

  // Update choices (dynamic count)
  choicesEl.dataset.count = gameState.activeTrivia.choices.length;
  choicesEl.innerHTML = gameState.activeTrivia.choices
    .map((choice, idx) => {
      const content = choice.image_url
        ? `<img src="${escapeHtml(choice.image_url)}" class="trivia-choice-image" alt="Choice ${TRIVIA_LABELS[idx]}">`
        : `<span class="trivia-choice-text">${escapeHtml(choice.text)}</span>`;
      return `
      <div class="trivia-choice">
        <span class="trivia-choice-label">${TRIVIA_LABELS[idx]}</span>
        ${content}
      </div>
    `;
    })
    .join("");

  // Show overlay
  overlay.style.display = "flex";
}

function showTriviaResult() {
  const overlay = document.getElementById("triviaOverlay");
  const resultOverlay = document.getElementById("triviaResultOverlay");
  const questionEl = document.getElementById("triviaResultQuestion");
  const questionImageEl = document.getElementById("triviaResultQuestionImage");
  const choicesEl = document.getElementById("triviaResultChoices");
  const totalVotesEl = document.getElementById("triviaTotalVotes");

  if (!resultOverlay || !questionEl || !choicesEl) return;

  // Hide question overlay
  if (overlay) overlay.style.display = "none";

  const result = gameState.triviaResult;

  // Update question
  questionEl.textContent = result.question;

  // Update question image
  if (questionImageEl) {
    if (result.image_url) {
      questionImageEl.src = result.image_url;
      questionImageEl.style.display = "block";
    } else {
      questionImageEl.style.display = "none";
    }
  }

  // Calculate max votes for bar width
  const maxVotes = Math.max(...result.vote_counts, 1);

  // Update choices with results (dynamic count)
  choicesEl.innerHTML = result.choices
    .map((choice, idx) => {
      const isCorrect = idx === result.correct_index;
      const voteCount = result.vote_counts[idx] || 0;
      const percent = (voteCount / maxVotes) * 100;
      const content = choice.image_url
        ? `<img src="${escapeHtml(choice.image_url)}" class="trivia-result-image" alt="Choice ${TRIVIA_LABELS[idx]}">${isCorrect ? " ✓" : ""}`
        : `${escapeHtml(choice.text)}${isCorrect ? " ✓" : ""}`;

      return `
        <div class="trivia-result-choice ${isCorrect ? "correct" : ""}">
          <span class="trivia-result-label">${TRIVIA_LABELS[idx]}</span>
          <span class="trivia-result-text">${content}</span>
          <div class="trivia-result-bar-container">
            <div class="trivia-result-bar" style="width: ${percent}%"></div>
          </div>
          <span class="trivia-result-count">${voteCount}</span>
        </div>
      `;
    })
    .join("");

  // Update total votes
  if (totalVotesEl) {
    totalVotesEl.textContent = `Gesamt: ${result.total_votes} Stimmen`;
  }

  // Show result overlay
  resultOverlay.style.display = "flex";
}

function hideTriviaOverlays() {
  const overlay = document.getElementById("triviaOverlay");
  const resultOverlay = document.getElementById("triviaResultOverlay");

  if (overlay) overlay.style.display = "none";
  if (resultOverlay) resultOverlay.style.display = "none";
}
