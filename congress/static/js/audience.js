/**
 * Audience-specific JavaScript
 */

let wsConn = null;
let voterToken = null;
let currentPhase = null;
let submissions = [];
let selectedAiAnswer = null;
let selectedFunnyAnswer = null;
let hasVoted = false;
let panicMode = false;
let audienceTimer = null;
let promptSubmissionExpanded = true;
const STORAGE_KEY = "gptdash_voter_token";

// Prompt voting state
let promptCandidates = [];
let selectedPrompt = null;
let hasPromptVoted = false;

// Initialize
function init() {
  // Check if voter token is in localStorage
  voterToken = localStorage.getItem(STORAGE_KEY);

  // Initialize timer
  audienceTimer = new CountdownTimer("audienceTimer");

  // Initialize prompt input event listener
  initPromptInput();

  // Connect to WebSocket with token for state recovery
  wsConn = new WSConnection(
    "audience",
    handleMessage,
    updateConnectionStatus,
    voterToken,
  );
  wsConn.connect();
}

function requireConnection(errorElementId) {
  if (!wsConn || !wsConn.isConnected()) {
    if (errorElementId) {
      showError(errorElementId, "Verbinde noch mit dem Spiel-Server...");
    }
    return false;
  }
  return true;
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      console.log("Welcome message:", message);
      if (message.game) {
        panicMode = message.game.panic_mode || false;
        updatePhase(message.game.phase);
        // Start timer if in VOTING phase with deadline
        if (
          message.game.phase === "VOTING" &&
          message.game.phase_deadline &&
          message.server_now
        ) {
          audienceTimer.start(message.game.phase_deadline, message.server_now);
        }
      }
      break;

    case "audience_state":
      // State recovery on reconnect
      console.log("Audience state recovery:", message);
      if (message.has_voted && message.current_vote) {
        hasVoted = true;
        selectedAiAnswer = message.current_vote.ai_pick;
        selectedFunnyAnswer = message.current_vote.funny_pick;

        // If we're in voting phase, show confirmed screen
        if (currentPhase === "VOTING") {
          showScreen("confirmedScreen");
          updateVoteSummary();
        }
      }
      // Auto-join if we have a token
      if (voterToken) {
        updateConnectionStatus(true, "Als Publikum beigetreten");
        if (currentPhase !== "VOTING" || !hasVoted) {
          showScreen("waitingScreen");
        }
      }
      break;

    case "phase":
      // Update timer for VOTING phase
      if (
        message.phase === "VOTING" &&
        message.deadline &&
        message.server_now
      ) {
        audienceTimer.start(message.deadline, message.server_now);
      } else {
        audienceTimer.stop();
        audienceTimer.hide();
        const timerEl = document.getElementById("audienceTimer");
        if (timerEl) timerEl.textContent = "--:--";
      }
      updatePhase(message.phase);
      break;

    case "deadline_update":
      // Update timer when deadline is extended
      if (audienceTimer && message.deadline && message.server_now) {
        audienceTimer.updateDeadline(message.deadline, message.server_now);
      }
      break;

    case "submissions":
      submissions = message.list || [];
      if (currentPhase === "VOTING") {
        renderAnswerOptions();
        updateVoteSummary();
      }
      break;

    case "vote_ack":
      console.log("Vote acknowledged");
      hasVoted = true;
      showScreen("confirmedScreen");
      updateVoteSummary();
      updateVoteButtonState();
      break;

    case "panic_mode_update":
      panicMode = message.enabled;
      updatePanicModeUI();
      break;

    case "prompt_candidates":
      // Received prompt options for voting
      promptCandidates = message.prompts || [];
      console.log("Prompt candidates received:", promptCandidates);
      if (currentPhase === "PROMPT_SELECTION" && promptCandidates.length > 1) {
        showPromptVotingScreen();
      }
      break;

    case "prompt_vote_ack":
      console.log("Prompt vote acknowledged");
      hasPromptVoted = true;
      showScreen("promptVoteConfirmedScreen");
      updatePromptVoteSummary();
      break;

    case "error":
      handleError(message.code, message.msg);
      break;
  }
}

function joinAudience() {
  // Generate voter token if we don't have one
  if (!voterToken) {
    voterToken = generateId("voter");
    localStorage.setItem(STORAGE_KEY, voterToken);
    // Update connection with token for future reconnects
    wsConn.setToken(voterToken);
  }

  if (!requireConnection("welcomeError")) {
    return;
  }

  // Send join message
  const sent = wsConn.send({
    t: "join",
    room_token: voterToken,
  });

  if (!sent) {
    showError("welcomeError", "Server nicht erreichbar. Versuch's nochmal.");
    return;
  }

  updateConnectionStatus(true, "Als Publikum beigetreten");
  hideError("welcomeError");
  showScreen("waitingScreen");
}

function updatePhase(phase) {
  console.log("Phase update:", phase);
  currentPhase = phase;

  switch (phase) {
    case "LOBBY":
      if (voterToken) {
        updateWaitingMessage(
          "&#x23F3;",
          "Willkommen!",
          "Die Show beginnt in Kürze. Schlage schon mal einen Prompt vor!",
        );
        showScreen("waitingScreen");
      }
      break;

    case "PROMPT_SELECTION":
      if (voterToken) {
        // Reset prompt voting state for new selection
        selectedPrompt = null;
        hasPromptVoted = false;
        // If we have multiple prompt candidates, show voting screen
        if (promptCandidates.length > 1) {
          showPromptVotingScreen();
        } else {
          updateWaitingMessage(
            "&#x1F4AC;",
            "Prompt-Auswahl",
            "Schau auf die große Leinwand - es wird ein Prompt ausgewählt!",
          );
          showScreen("waitingScreen");
        }
      }
      break;

    case "WRITING":
      if (voterToken) {
        updateWaitingMessage(
          "&#x270D;&#xFE0F;",
          "Spieler schreiben...",
          "Die Spieler denken sich gerade ihre Antworten aus. Gleich kannst du abstimmen!",
        );
        showScreen("waitingScreen");
      }
      break;

    case "REVEAL":
      if (voterToken) {
        updateWaitingMessage(
          "&#x1F440;",
          "Antworten werden enthüllt",
          "Schau auf die große Leinwand - die Antworten werden vorgelesen!",
        );
        showScreen("waitingScreen");
      }
      break;

    case "VOTING":
      showVotingScreen();
      break;

    case "RESULTS":
      showScreen("resultsScreen");
      // Reset vote selections for next round
      selectedAiAnswer = null;
      selectedFunnyAnswer = null;
      hasVoted = false;
      break;

    case "PODIUM":
      if (voterToken) {
        updateWaitingMessage(
          "&#x1F3C6;",
          "Siegerehrung",
          "Die Gewinner werden verkündet! Schau auf die große Leinwand.",
        );
        showScreen("waitingScreen");
      }
      break;

    case "INTERMISSION":
      if (voterToken) {
        updateWaitingMessage(
          "&#x2615;",
          "Pause",
          "Kurze Pause. Gleich geht es weiter!",
        );
        showScreen("waitingScreen");
      }
      break;
  }
}

function updateWaitingMessage(icon, title, message) {
  const iconEl = document.getElementById("waitingIcon");
  const titleEl = document.getElementById("waitingTitle");
  const messageEl = document.getElementById("waitingMessage");

  if (iconEl) iconEl.innerHTML = icon;
  if (titleEl) titleEl.textContent = title;
  if (messageEl) messageEl.textContent = message;
}

function showVotingScreen() {
  renderAnswerOptions();
  hideError("voteError");
  showScreen("votingScreen");
  updateVoteSummary();
  updateVoteButtonState();
  updatePanicModeUI();
}

function renderAnswerOptions() {
  const aiContainer = document.getElementById("aiAnswerOptions");
  const funnyContainer = document.getElementById("funnyAnswerOptions");

  aiContainer.innerHTML = "";
  funnyContainer.innerHTML = "";

  if (submissions.length === 0) {
    const placeholder =
      '<p class="help-text" style="width: 100%; text-align: center;">Warte auf Antworten...</p>';
    aiContainer.innerHTML = placeholder;
    funnyContainer.innerHTML = placeholder;
    return;
  }

  submissions.forEach((sub, idx) => {
    aiContainer.appendChild(createAnswerOption("ai", sub, idx));
    funnyContainer.appendChild(createAnswerOption("funny", sub, idx));
  });
}

function createAnswerOption(category, sub, index) {
  const option = document.createElement("div");
  option.className = "answer-option";
  option.dataset.answerId = sub.id;
  option.innerHTML = `
        <div class="checkmark">✓</div>
        <div class="number">Antwort ${index + 1}</div>
        <div class="text">${escapeHtml(sub.display_text)}</div>
    `;

  if (
    (category === "ai" && selectedAiAnswer === sub.id) ||
    (category === "funny" && selectedFunnyAnswer === sub.id)
  ) {
    option.classList.add("selected");
  }

  option.addEventListener("click", () => selectAnswer(category, sub.id));

  return option;
}

function selectAnswer(category, answerId) {
  console.log(`Selected ${category}:`, answerId);

  if (category === "ai") {
    selectedAiAnswer = answerId;
    document
      .querySelectorAll("#aiAnswerOptions .answer-option")
      .forEach((opt) => {
        opt.classList.toggle("selected", opt.dataset.answerId === answerId);
      });
  } else {
    selectedFunnyAnswer = answerId;
    document
      .querySelectorAll("#funnyAnswerOptions .answer-option")
      .forEach((opt) => {
        opt.classList.toggle("selected", opt.dataset.answerId === answerId);
      });
  }

  updateVoteButtonState();
}

function updateVoteButtonState() {
  const voteButton = document.getElementById("voteButton");
  voteButton.disabled =
    panicMode || hasVoted || !(selectedAiAnswer && selectedFunnyAnswer);
}

function submitVote() {
  if (hasVoted) {
    showError("voteError", "Du hast in dieser Runde schon abgestimmt");
    return;
  }

  if (!selectedAiAnswer || !selectedFunnyAnswer) {
    showError("voteError", "Bitte wähle eine Antwort für beide Kategorien");
    return;
  }

  if (!requireConnection("voteError")) {
    return;
  }

  // Generate message ID for idempotency
  const msgId = generateId("msg");

  // Send vote
  const sent = wsConn.send({
    t: "vote",
    voter_token: voterToken,
    ai: selectedAiAnswer,
    funny: selectedFunnyAnswer,
    msg_id: msgId,
  });

  if (!sent) {
    showError("voteError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  hasVoted = true;
  updateVoteButtonState();
  hideError("voteError");
}

function changeVote() {
  hasVoted = false;
  showVotingScreen();
}

function updateVoteSummary() {
  const aiIndex = submissions.findIndex((s) => s.id === selectedAiAnswer);
  const funnyIndex = submissions.findIndex((s) => s.id === selectedFunnyAnswer);

  const aiLabel = aiIndex >= 0 ? `Antwort ${aiIndex + 1}` : "—";
  const funnyLabel = funnyIndex >= 0 ? `Antwort ${funnyIndex + 1}` : "—";

  document.getElementById("summaryAiPick").textContent = aiLabel;
  document.getElementById("summaryFunnyPick").textContent = funnyLabel;
}

function handleError(code, message) {
  console.error("Server error:", code, message);

  // Handle panic mode error specially
  if (code === "PANIC_MODE") {
    panicMode = true;
    updatePanicModeUI();
    return;
  }

  if (document.getElementById("welcomeScreen").classList.contains("active")) {
    showError("welcomeError", message);
  } else {
    showError("voteError", message);
  }
}

function updatePanicModeUI() {
  const panicOverlay = document.getElementById("panicModeOverlay");
  const votingContent = document.getElementById("votingContent");
  const voteButton = document.getElementById("voteButton");

  if (panicOverlay) {
    panicOverlay.style.display = panicMode ? "flex" : "none";
  }

  if (votingContent) {
    votingContent.style.opacity = panicMode ? "0.3" : "1";
    votingContent.style.pointerEvents = panicMode ? "none" : "auto";
  }

  if (voteButton) {
    voteButton.disabled =
      panicMode || hasVoted || !(selectedAiAnswer && selectedFunnyAnswer);
  }
}

// Prompt submission functions
function togglePromptSubmission() {
  promptSubmissionExpanded = !promptSubmissionExpanded;
  const content = document.getElementById("promptSubmissionContent");
  const icon = document.getElementById("promptToggleIcon");

  if (content) {
    content.classList.toggle("collapsed", !promptSubmissionExpanded);
  }
  if (icon) {
    icon.classList.toggle("collapsed", !promptSubmissionExpanded);
  }
}

function updatePromptCharCount() {
  const input = document.getElementById("promptInput");
  const counter = document.getElementById("promptCharCount");
  if (input && counter) {
    counter.textContent = input.value.length;
  }
}

function submitPrompt() {
  const input = document.getElementById("promptInput");
  const text = input?.value?.trim();

  if (!text) {
    showError("promptError", "Bitte gib einen Prompt ein");
    return;
  }

  if (text.length < 10) {
    showError("promptError", "Der Prompt sollte mindestens 10 Zeichen haben");
    return;
  }

  if (!requireConnection("promptError")) {
    return;
  }

  // Send prompt submission
  const sent = wsConn.send({
    t: "submit_prompt",
    voter_token: voterToken,
    text: text,
  });

  if (!sent) {
    showError("promptError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  // Show success feedback
  hideError("promptError");
  const successEl = document.getElementById("promptSuccess");
  if (successEl) {
    successEl.style.display = "block";
    // Hide after 3 seconds
    setTimeout(() => {
      successEl.style.display = "none";
    }, 3000);
  }

  // Clear input
  if (input) {
    input.value = "";
    updatePromptCharCount();
  }
}

function initPromptInput() {
  const input = document.getElementById("promptInput");
  if (input) {
    input.addEventListener("input", updatePromptCharCount);
  }
}

// Prompt voting functions
function showPromptVotingScreen() {
  renderPromptOptions();
  hideError("promptVoteError");
  showScreen("promptVotingScreen");
  updatePromptVoteButtonState();
}

function renderPromptOptions() {
  const container = document.getElementById("promptOptions");
  if (!container) return;

  container.innerHTML = "";

  if (promptCandidates.length === 0) {
    container.innerHTML =
      '<p class="help-text" style="width: 100%; text-align: center;">Warte auf Prompts...</p>';
    return;
  }

  promptCandidates.forEach((prompt, idx) => {
    const option = document.createElement("div");
    option.className = "prompt-option";
    option.dataset.promptId = prompt.id;

    let content = `<div class="checkmark">✓</div><div class="number">Prompt ${idx + 1}</div>`;

    // Add image if present
    if (prompt.image_url) {
      content += `<img src="${escapeHtml(prompt.image_url)}" class="prompt-image" alt="Prompt Bild" onerror="this.style.display='none'">`;
    }

    // Add text if present
    if (prompt.text) {
      content += `<div class="text">${escapeHtml(prompt.text)}</div>`;
    }

    option.innerHTML = content;

    if (selectedPrompt === prompt.id) {
      option.classList.add("selected");
    }

    option.addEventListener("click", () => selectPromptOption(prompt.id));
    container.appendChild(option);
  });
}

function selectPromptOption(promptId) {
  console.log("Selected prompt:", promptId);
  selectedPrompt = promptId;

  // Update UI
  document.querySelectorAll("#promptOptions .prompt-option").forEach((opt) => {
    opt.classList.toggle("selected", opt.dataset.promptId === promptId);
  });

  updatePromptVoteButtonState();
}

function updatePromptVoteButtonState() {
  const button = document.getElementById("promptVoteButton");
  if (button) {
    button.disabled = !selectedPrompt || hasPromptVoted;
  }
}

function submitPromptVote() {
  if (hasPromptVoted) {
    showError("promptVoteError", "Du hast in dieser Runde schon abgestimmt");
    return;
  }

  if (!selectedPrompt) {
    showError("promptVoteError", "Bitte wähle einen Prompt aus");
    return;
  }

  if (!requireConnection("promptVoteError")) {
    return;
  }

  // Generate message ID for idempotency
  const msgId = generateId("msg");

  // Send prompt vote
  const sent = wsConn.send({
    t: "prompt_vote",
    voter_token: voterToken,
    prompt_id: selectedPrompt,
    msg_id: msgId,
  });

  if (!sent) {
    showError("promptVoteError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  hasPromptVoted = true;
  updatePromptVoteButtonState();
  hideError("promptVoteError");
}

function changePromptVote() {
  hasPromptVoted = false;
  showPromptVotingScreen();
}

function updatePromptVoteSummary() {
  const summaryEl = document.getElementById("promptVoteSummary");
  if (!summaryEl) return;

  const prompt = promptCandidates.find((p) => p.id === selectedPrompt);
  if (prompt) {
    const text = prompt.text
      ? prompt.text.substring(0, 50) + (prompt.text.length > 50 ? "..." : "")
      : "(Bild-Prompt)";
    summaryEl.textContent = `"${text}"`;
  } else {
    summaryEl.textContent = "";
  }
}

if (typeof window !== "undefined") {
  Object.assign(window, {
    joinAudience,
    submitVote,
    changeVote,
    togglePromptSubmission,
    submitPrompt,
    submitPromptVote,
    changePromptVote,
  });
}

// Initialize on page load
init();
