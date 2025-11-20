/**
 * Player-specific JavaScript
 */

let wsConn = null;
let playerToken = null;
let playerName = null;
let currentPhase = null;
let currentPrompt = null;
const MAX_CHARS = 500;
const WARN_THRESHOLD = 450;

// Initialize
function init() {
  // Check if player token is in URL
  const params = new URLSearchParams(window.location.search);
  const urlToken = params.get("token");
  if (urlToken) {
    document.getElementById("tokenInput").value = urlToken;
  }

  // Setup char counter
  const answerInput = document.getElementById("answerInput");
  if (answerInput) {
    answerInput.addEventListener("input", updateCharCounter);
  }

  // Connect to WebSocket
  wsConn = new WSConnection("player", handleMessage, updateConnectionStatus);
  wsConn.connect();
}

function requireConnection(errorElementId) {
  if (!wsConn || !wsConn.isConnected()) {
    if (errorElementId) {
      showError(errorElementId, "Still connecting to the game server...");
    }
    return false;
  }
  return true;
}

function handleMessage(message) {
  switch (message.t) {
    case "welcome":
      console.log("Welcome message:", message);
      break;

    case "phase":
      currentPhase = message.phase;
      updateScreen(message.phase);
      break;

    case "round_started":
      currentPrompt = message.prompt;
      if (currentPhase === "WRITING") {
        showWritingScreen();
      }
      break;

    case "submission_confirmed":
      showScreen("submittedScreen");
      break;

    case "error":
      handleError(message.msg);
      break;
  }
}

function joinGame() {
  const tokenInput = document.getElementById("tokenInput");
  const token = tokenInput.value.trim();

  if (!token) {
    showError("joinError", "Please enter a player token");
    return;
  }

  playerToken = token;

  if (!requireConnection("joinError")) {
    return;
  }

  // Send join message
  const sent = wsConn.send({
    t: "join",
    room_token: token,
  });

  if (!sent) {
    showError(
      "joinError",
      "Failed to reach the server. Please try again in a moment.",
    );
    return;
  }

  hideError("joinError");

  // Move to register screen
  showScreen("registerScreen");
}

function registerName() {
  const nameInput = document.getElementById("nameInput");
  const name = nameInput.value.trim();

  if (!name) {
    showError("registerError", "Please enter a display name");
    return;
  }

  if (name.length < 2) {
    showError("registerError", "Name must be at least 2 characters");
    return;
  }

  playerName = name;

  if (!requireConnection("registerError")) {
    return;
  }

  // Send register message
  const sent = wsConn.send({
    t: "register_player",
    player_token: playerToken,
    display_name: name,
  });

  if (!sent) {
    showError("registerError", "Connection lost. Please try again.");
    return;
  }

  // Update status
  updateConnectionStatus(true, `Joined as ${name}`);
  hideError("registerError");

  // Show waiting screen
  showScreen("waitingScreen");
}

function submitAnswer() {
  const answerInput = document.getElementById("answerInput");
  const answer = answerInput.value.trim();

  if (!answer) {
    showError("submitError", "Please enter an answer");
    return;
  }

  if (answer.length < 10) {
    showError("submitError", "Answer must be at least 10 characters");
    return;
  }

  if (!requireConnection("submitError")) {
    return;
  }

  // Send submission
  const sent = wsConn.send({
    t: "submit_answer",
    player_token: playerToken,
    text: answer,
  });

  if (!sent) {
    showError("submitError", "Connection lost. Please try again.");
    return;
  }

  // Show submitted screen
  showScreen("submittedScreen");
  hideError("submitError");
}

function editAnswer() {
  showScreen("writingScreen");
}

function updateScreen(phase) {
  console.log("Phase update:", phase);

  switch (phase) {
    case "LOBBY":
    case "PROMPT_SELECTION":
      if (playerName) {
        showScreen("waitingScreen");
      }
      break;

    case "WRITING":
      showWritingScreen();
      break;

    case "REVEAL":
    case "VOTING":
    case "RESULTS":
    case "PODIUM":
      showScreen("lockedScreen");
      break;

    case "INTERMISSION":
      showScreen("waitingScreen");
      break;
  }
}

function showWritingScreen() {
  if (currentPrompt) {
    const promptEl = document.getElementById("promptText");
    if (promptEl) {
      promptEl.textContent = currentPrompt.text || currentPrompt;
    }
  }
  showScreen("writingScreen");
}

function updateCharCounter() {
  const answerInput = document.getElementById("answerInput");
  const counter = document.getElementById("charCounter");
  const length = answerInput.value.length;

  counter.textContent = `${length} / ${MAX_CHARS}`;

  counter.classList.remove("warning", "error");

  if (length >= MAX_CHARS) {
    counter.classList.add("error");
  } else if (length >= WARN_THRESHOLD) {
    counter.classList.add("warning");
  }
}

function handleError(message) {
  console.error("Server error:", message);
  // Show error in current screen context
  const currentScreen = document.querySelector(".screen.active");
  if (currentScreen) {
    const errorEl = currentScreen.querySelector(".error-message");
    if (errorEl) {
      showError(errorEl.id, message);
    }
  }
}

if (typeof window !== "undefined") {
  Object.assign(window, {
    joinGame,
    registerName,
    submitAnswer,
    editAnswer,
  });
}

// Initialize on page load
init();
