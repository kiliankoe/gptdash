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
const STORAGE_KEY = "gptdash_voter_token";

// Initialize
function init() {
  // Check if voter token is in localStorage
  voterToken = localStorage.getItem(STORAGE_KEY);

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
        updatePhase(message.game.phase);
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
      updatePhase(message.phase);
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

    case "error":
      handleError(message.msg);
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
    case "PROMPT_SELECTION":
    case "WRITING":
    case "REVEAL":
      if (voterToken) {
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
    case "INTERMISSION":
      if (voterToken) {
        showScreen("waitingScreen");
      }
      break;
  }
}

function showVotingScreen() {
  renderAnswerOptions();
  hideError("voteError");
  showScreen("votingScreen");
  updateVoteSummary();
  updateVoteButtonState();
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
  voteButton.disabled = hasVoted || !(selectedAiAnswer && selectedFunnyAnswer);
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

function handleError(message) {
  console.error("Server error:", message);
  if (document.getElementById("welcomeScreen").classList.contains("active")) {
    showError("welcomeError", message);
  } else {
    showError("voteError", message);
  }
}

if (typeof window !== "undefined") {
  Object.assign(window, {
    joinAudience,
    submitVote,
    changeVote,
  });
}

// Initialize on page load
init();
