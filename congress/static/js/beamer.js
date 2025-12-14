/**
 * GPTDash Beamer Display - 39C3 Edition
 * Full-screen stage display for projector/TV
 */

// Game state
const gameState = {
  phase: "LOBBY",
  roundNo: 0,
  currentRound: null,
  submissions: [],
  revealIndex: 0,
  currentRevealSubmission: null,
  scores: { players: [], audienceTop: [] },
  voteCounts: { ai: {}, funny: {} },
  promptCandidates: [], // Prompts for voting during PROMPT_SELECTION
  promptVoteCounts: {}, // Vote counts per prompt during PROMPT_SELECTION
};

// Connections and utilities
let ws = null;
let timer = null;
let tts = null;
let lastSpokenSubmissionId = null;

// Initialize on page load
document.addEventListener("DOMContentLoaded", () => {
  initializeBeamer();
});

function initializeBeamer() {
  // Initialize TTS
  tts = new TTSManager();

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
  const displayUrl = url.replace(/^https?:\/\//, "");
  const lobbyUrlEl = document.getElementById("lobbyUrl");
  const headerUrlEl = document.getElementById("headerUrl");
  if (lobbyUrlEl) lobbyUrlEl.textContent = displayUrl;
  if (headerUrlEl) headerUrlEl.textContent = displayUrl;
}

function connectWebSocket() {
  ws = new WSConnection("beamer", handleMessage, handleStatusChange);
  ws.connect();
}

function handleStatusChange(connected, text) {
  const dot = document.getElementById("statusDot");
  const statusText = document.getElementById("statusText");

  if (dot) {
    if (connected) {
      dot.classList.add("connected");
    } else {
      dot.classList.remove("connected");
    }
  }

  if (statusText) {
    statusText.textContent =
      text || (connected ? "Verbunden" : "Nicht verbunden");
  }
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
    lastSpokenSubmissionId = null;
  }

  showScene(phaseToScene(msg.phase));
}

function handleRoundStarted(msg) {
  gameState.currentRound = msg.round;
  gameState.roundNo = msg.round.number;
  updateRoundBadge();

  // Update prompt candidates for selection
  if (msg.round.prompt_candidates && msg.round.prompt_candidates.length > 0) {
    updatePromptCandidates(msg.round.prompt_candidates);
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

  if (promptText) {
    promptText.textContent = prompt.text || "(Bildfrage)";
    // If image-only, hide the text element
    promptText.style.display = prompt.text ? "block" : "none";
  }

  if (promptImage) {
    if (prompt.image_url) {
      promptImage.innerHTML = `<img src="${escapeHtml(prompt.image_url)}" alt="Prompt-Bild" class="prompt-image-display">`;
      promptImage.style.display = "block";
    } else {
      promptImage.innerHTML = "";
      promptImage.style.display = "none";
    }
  }
}

function handleSubmissions(msg) {
  gameState.submissions = msg.list || [];
  updateSubmissionCounter();

  // Reinitialize vote bars if we're in voting phase
  if (gameState.phase === "VOTING") {
    initVotingBars();
    updateVoteBarsAnimated();
  }

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
  }
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
    const count = gameState.submissions.length;
    const text =
      count === 1 ? "1 Antwort eingereicht" : `${count} Antworten eingereicht`;
    counter.textContent = text;
  }
}

function initVotingBars() {
  const submissions = gameState.submissions;
  if (submissions.length === 0) return;

  const aiContainer = document.getElementById("aiVoteBars");
  const funnyContainer = document.getElementById("funnyVoteBars");

  if (!aiContainer || !funnyContainer) return;

  aiContainer.innerHTML = "";
  funnyContainer.innerHTML = "";

  submissions.forEach((sub, index) => {
    const label = String.fromCharCode(65 + index); // A, B, C, ...

    // AI vote bar
    aiContainer.appendChild(createVoteBar(sub.id, label, "ai"));
    // Funny vote bar
    funnyContainer.appendChild(createVoteBar(sub.id, label, "funny"));
  });
}

function createVoteBar(id, label, type) {
  const bar = document.createElement("div");
  bar.className = "vote-bar";
  bar.dataset.submissionId = id;
  bar.innerHTML = `
        <div class="vote-bar-label">${escapeHtml(label)}</div>
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

  const count = gameState.submissions.length;
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
    const total = gameState.submissions.length;
    numberEl.textContent = `Antwort ${gameState.revealIndex + 1} von ${total}`;
    textEl.textContent = gameState.currentRevealSubmission.display_text;
  } else {
    numberEl.textContent = "Warte auf Antworten";
    textEl.textContent =
      "Die Antworten erscheinen hier, sobald der Host die Praesentation startet.";
  }
}

function showRevealCard(submission, index) {
  const total = gameState.submissions.length;
  const numberEl = document.getElementById("revealNumber");
  const textEl = document.getElementById("revealText");

  if (numberEl) numberEl.textContent = `Antwort ${index + 1} von ${total}`;
  if (textEl) textEl.textContent = submission.display_text;

  // Animate card
  const card = document.getElementById("revealCard");
  if (card) {
    card.style.animation = "none";
    card.offsetHeight; // Trigger reflow
    card.style.animation = "slideUp 0.6s ease-out";
  }

  // Speak the answer with TTS (only if we haven't spoken this one yet)
  if (submission.id !== lastSpokenSubmissionId) {
    lastSpokenSubmissionId = submission.id;
    setTimeout(() => {
      tts.speak(submission.display_text, {
        rate: 0.9,
        pitch: 1.0,
      });
    }, 600);
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

  container.innerHTML = sorted
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

  // Show top 5 audience members
  container.innerHTML = sorted
    .slice(0, 5)
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
}

function updateResultReveals() {
  const aiCounts = gameState.voteCounts.ai;
  const funnyCounts = gameState.voteCounts.funny;

  // Find most voted AI
  let maxAiVotes = 0;
  let aiWinnerId = null;
  for (const [id, count] of Object.entries(aiCounts)) {
    if (count > maxAiVotes) {
      maxAiVotes = count;
      aiWinnerId = id;
    }
  }

  // Find most voted Funny
  let maxFunnyVotes = 0;
  let funnyWinnerId = null;
  for (const [id, count] of Object.entries(funnyCounts)) {
    if (count > maxFunnyVotes) {
      maxFunnyVotes = count;
      funnyWinnerId = id;
    }
  }

  // Update AI reveal (use actual AI submission if marked by host)
  const aiRevealAnswer = document.getElementById("aiRevealAnswer");
  const aiRevealMeta = document.getElementById("aiRevealMeta");

  // Check if we have a designated AI submission from the round
  const actualAiId = gameState.currentRound?.ai_submission_id;
  const aiDisplayId = actualAiId || aiWinnerId;

  if (aiDisplayId && aiRevealAnswer) {
    const aiSub = gameState.submissions.find((s) => s.id === aiDisplayId);
    if (aiSub) {
      aiRevealAnswer.textContent = aiSub.display_text;
      if (aiRevealMeta) {
        const votes = aiCounts[aiDisplayId] || 0;
        aiRevealMeta.textContent = `${votes} Stimmen`;
      }
    }
  }

  // Update Funny reveal
  const funnyRevealAnswer = document.getElementById("funnyRevealAnswer");
  const funnyRevealMeta = document.getElementById("funnyRevealMeta");

  if (funnyWinnerId && funnyRevealAnswer) {
    const funnySub = gameState.submissions.find((s) => s.id === funnyWinnerId);
    if (funnySub) {
      funnyRevealAnswer.textContent = funnySub.display_text;
      if (funnyRevealMeta) {
        funnyRevealMeta.textContent = `${maxFunnyVotes} Stimmen`;
      }
    }
  }
}

function updatePodium() {
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

// Sync phase-specific timers based on current phase
setInterval(() => {
  // Get the active timer element based on phase
  let activeTimerEl = null;
  if (gameState.phase === "WRITING") {
    activeTimerEl = document.getElementById("writingTimer");
  } else if (gameState.phase === "VOTING") {
    activeTimerEl = document.getElementById("votingTimer");
  }

  if (!activeTimerEl) return;

  const time = activeTimerEl.textContent;
  if (!time || time === "") return;

  // Parse time to check for warning threshold
  const parts = time.split(":");
  const minutes = parseInt(parts[0] || "0", 10);
  const seconds = parseInt(parts[1] || "0", 10);
  const isWarning = minutes === 0 && seconds <= 10;

  if (isWarning) {
    activeTimerEl.classList.add("warning");
  } else {
    activeTimerEl.classList.remove("warning");
  }
}, 100);
