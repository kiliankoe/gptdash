/**
 * Audience-specific JavaScript
 */

import {
  WSConnection,
  CountdownTimer,
  ChallengeSolver,
  generateId,
  escapeHtml,
  showScreen,
  showError,
  hideError,
  updateConnectionStatus,
} from "./common.js";

let wsConn = null;
let voterToken = null;
let displayName = null; // Auto-generated friendly name from server
let currentPhase = null;
let currentRoundNo = null;
let submissions = [];
let selectedAiAnswer = null;
let selectedFunnyAnswer = null;
let hasVoted = false;
let audienceTimer = null;
let promptSubmissionExpanded = false;
let challengeSolver = null; // Vote challenge solver (anti-automation)
const STORAGE_KEY = "gptdash_voter_token";

// Prompt voting state
let promptCandidates = [];
let selectedPrompt = null;
let hasPromptVoted = false;

// Trivia state
let triviaQuestion = null; // Current trivia question (id, question, choices)
let selectedTriviaChoice = null;
let hasTriviaVoted = false;
let triviaResult = null; // Trivia result after resolve
const TRIVIA_LABELS = ["A", "B", "C", "D"]; // Dynamic labels for 2-4 choices

// Leaderboard state for winner detection
let audienceLeaderboard = [];

// Initialize
function init() {
  // Check if voter token is in localStorage
  voterToken = localStorage.getItem(STORAGE_KEY);
  // Ensure we always have a token before connecting (server requires it)
  if (!voterToken) {
    voterToken = generateId("voter");
    localStorage.setItem(STORAGE_KEY, voterToken);
  }

  // Initialize timer
  audienceTimer = new CountdownTimer("audienceTimer");

  // Initialize vote challenge solver (anti-automation)
  challengeSolver = new ChallengeSolver();

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
        currentRoundNo = message.game.round_no;
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

      // Store and display the friendly name
      if (message.display_name) {
        displayName = message.display_name;
        updateDisplayNameUI();
      }

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
      // Don't override the current phase screen (e.g. a late-joining audience member
      // should stay on the voting screen and receive submissions via state recovery).
      // Only force confirmed screen if we already voted in the current voting phase.
      if (currentPhase === "VOTING" && hasVoted) {
        showScreen("confirmedScreen");
        updateVoteSummary();
      }
      break;

    case "phase":
      // Detect new round via phase message (covers implicit round starts)
      if (
        typeof message.round_no === "number" &&
        currentRoundNo !== null &&
        message.round_no !== currentRoundNo
      ) {
        resetRoundUiState();
      }
      if (typeof message.round_no === "number") {
        currentRoundNo = message.round_no;
      }
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

    case "audience_prompt_vote_state":
      // State recovery for prompt voting on reconnect
      console.log("Audience prompt vote state recovery:", message);
      if (message.has_voted && message.voted_prompt_id) {
        hasPromptVoted = true;
        selectedPrompt = message.voted_prompt_id;
        // If we're in prompt selection phase, show confirmed screen
        if (currentPhase === "PROMPT_SELECTION") {
          showScreen("promptVoteConfirmedScreen");
          updatePromptVoteSummary();
        }
      }
      break;

    case "vote_challenge":
      // Store vote challenge for anti-automation
      console.log("Vote challenge received:", message);
      if (challengeSolver && message.nonce) {
        challengeSolver.setChallenge(message.nonce, message.round_id);
      }
      break;

    case "scores":
      // Store leaderboard for winner detection in PODIUM phase
      console.log("Scores received:", message);
      audienceLeaderboard = message.audience_top || [];
      checkWinnerStatus();
      break;

    // Trivia messages
    case "trivia_question":
      handleTriviaQuestion(message);
      break;

    case "trivia_vote_ack":
      console.log("Trivia vote acknowledged:", message);
      hasTriviaVoted = true;
      showScreen("triviaConfirmedScreen");
      updateTriviaVoteSummary();
      break;

    case "trivia_vote_state":
      // State recovery for trivia voting on reconnect
      console.log("Trivia vote state recovery:", message);
      if (message.has_voted && message.choice_index !== null) {
        hasTriviaVoted = true;
        selectedTriviaChoice = message.choice_index;
        // If trivia is active and we're in WRITING phase, show confirmed screen
        if (triviaQuestion && currentPhase === "WRITING") {
          showScreen("triviaConfirmedScreen");
          updateTriviaVoteSummary();
        }
      }
      break;

    case "trivia_result":
      handleTriviaResult(message);
      break;

    case "trivia_clear":
      handleTriviaClear();
      break;

    case "error":
      handleError(message.code, message.msg);
      break;
  }
}

function resetRoundUiState() {
  submissions = [];
  hasVoted = false;
  selectedAiAnswer = null;
  selectedFunnyAnswer = null;
  promptCandidates = [];
  hasPromptVoted = false;
  selectedPrompt = null;
  // Clear trivia state
  triviaQuestion = null;
  triviaResult = null;
  hasTriviaVoted = false;
  selectedTriviaChoice = null;
  // Clear challenge for new round
  if (challengeSolver) {
    challengeSolver.clear();
  }
  updateVoteButtonState();
}

function joinAudience() {
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

  updateConnectionStatus(true, "connected");
  hideError("welcomeError");

  // Show the appropriate screen based on current phase (important for late joiners)
  if (currentPhase === "VOTING" && !hasVoted) {
    showVotingScreen();
  } else if (currentPhase === "WRITING" && triviaQuestion && !hasTriviaVoted) {
    // If there's an active trivia question during WRITING, show trivia voting
    showTriviaScreen();
  } else if (currentPhase === "WRITING" && triviaQuestion && hasTriviaVoted) {
    // If they've already voted on trivia, show confirmed screen
    showScreen("triviaConfirmedScreen");
    updateTriviaVoteSummary();
  } else if (triviaResult) {
    // If trivia results are showing, display them
    showTriviaResultScreen();
  } else {
    showScreen("waitingScreen");
  }
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
          "Die Show beginnt in Kürze!",
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
        if (isInTopThree()) {
          showScreen("winnerFullscreen");
        } else {
          updateWaitingMessage(
            "&#x1F3C6;",
            "Siegerehrung",
            "Die Gewinner werden verkündet! Schau auf die große Leinwand.",
          );
          showScreen("waitingScreen");
        }
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

function updateDisplayNameUI() {
  // Update all elements that show the display name
  const nameElements = document.querySelectorAll(".audience-display-name");
  nameElements.forEach((el) => {
    el.textContent = displayName || "";
    el.style.display = displayName ? "block" : "none";
  });

  // Update the header name display
  const headerName = document.getElementById("headerDisplayName");
  if (headerName) {
    headerName.textContent = displayName || "";
    headerName.style.display = displayName ? "inline" : "none";
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

async function submitVote() {
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

  // Solve vote challenge (anti-automation)
  let challenge;
  try {
    if (!challengeSolver || !challengeSolver.hasChallenge()) {
      showError("voteError", "Technischer Fehler. Bitte Seite neu laden.");
      return;
    }
    challenge = await challengeSolver.solve(voterToken);
  } catch (e) {
    console.error("Challenge solve failed:", e);
    showError("voteError", "Technischer Fehler. Bitte Seite neu laden.");
    return;
  }

  // Generate message ID for idempotency
  const msgId = generateId("msg");

  // Check if running under automation (navigator.webdriver)
  const isWebdriver = navigator.webdriver === true;

  // Send vote with challenge response and anti-automation fields
  const sent = wsConn.send({
    t: "vote",
    voter_token: voterToken,
    ai: selectedAiAnswer,
    funny: selectedFunnyAnswer,
    msg_id: msgId,
    challenge_nonce: challenge.nonce,
    challenge_response: challenge.response,
    is_webdriver: isWebdriver,
  });

  if (!sent) {
    showError("voteError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  hasVoted = true;
  updateVoteButtonState();
  hideError("voteError");
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

  if (document.getElementById("welcomeScreen").classList.contains("active")) {
    showError("welcomeError", message);
  } else {
    showError("voteError", message);
  }
}

// Winner detection for PODIUM phase
function isInTopThree() {
  if (!voterToken || !audienceLeaderboard || audienceLeaderboard.length === 0) {
    return false;
  }
  const top3 = audienceLeaderboard.slice(0, 3);
  return top3.some((score) => score.ref_id === voterToken);
}

function checkWinnerStatus() {
  // If we're in PODIUM and now know we're a winner, show the winner screen
  if (currentPhase === "PODIUM" && isInTopThree()) {
    showScreen("winnerFullscreen");
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

// ========================
// Trivia Functions
// ========================

function handleTriviaQuestion(message) {
  console.log("Trivia question received:", message);
  triviaQuestion = {
    question_id: message.question_id,
    question: message.question,
    choices: message.choices,
  };
  triviaResult = null;
  hasTriviaVoted = false;
  selectedTriviaChoice = null;

  // If we're in WRITING phase, show trivia screen
  if (currentPhase === "WRITING") {
    showTriviaScreen();
  }
}

function handleTriviaResult(message) {
  console.log("Trivia result received:", message);
  triviaResult = {
    question_id: message.question_id,
    question: message.question,
    choices: message.choices,
    correct_index: message.correct_index,
    vote_counts: message.vote_counts,
    total_votes: message.total_votes,
  };
  triviaQuestion = null; // Clear active question

  // Show result screen
  showTriviaResultScreen();
}

function handleTriviaClear() {
  console.log("Trivia cleared");
  triviaQuestion = null;
  triviaResult = null;
  hasTriviaVoted = false;
  selectedTriviaChoice = null;

  // Return to waiting screen if in WRITING phase
  if (currentPhase === "WRITING") {
    updateWaitingMessage(
      "&#x270D;&#xFE0F;",
      "Spieler schreiben...",
      "Die Spieler denken sich gerade ihre Antworten aus. Gleich kannst du abstimmen!",
    );
    showScreen("waitingScreen");
  }
}

function showTriviaScreen() {
  renderTriviaOptions();
  hideError("triviaError");
  showScreen("triviaScreen");
}

function renderTriviaOptions() {
  const questionEl = document.getElementById("triviaQuestionText");
  const container = document.getElementById("triviaOptions");

  if (!questionEl || !container || !triviaQuestion) return;

  // Update question text
  questionEl.textContent = triviaQuestion.question;

  // Render choices (dynamic count 2-4)
  container.innerHTML = "";

  triviaQuestion.choices.forEach((choice, idx) => {
    const option = document.createElement("div");
    option.className = "trivia-option";
    option.dataset.choiceIndex = idx;
    option.innerHTML = `
      <span class="checkmark">✓</span>
      <span class="trivia-label">${TRIVIA_LABELS[idx]}</span>
      <span class="trivia-text">${escapeHtml(choice)}</span>
    `;

    if (selectedTriviaChoice === idx) {
      option.classList.add("selected");
    }

    option.addEventListener("click", () => selectTriviaChoice(idx));
    container.appendChild(option);
  });
}

function selectTriviaChoice(choiceIndex) {
  console.log("Selected trivia choice:", choiceIndex);
  selectedTriviaChoice = choiceIndex;

  // Update UI
  document.querySelectorAll("#triviaOptions .trivia-option").forEach((opt) => {
    opt.classList.toggle(
      "selected",
      parseInt(opt.dataset.choiceIndex, 10) === choiceIndex,
    );
  });

  // Auto-submit vote (no need for submit button like main voting)
  submitTriviaVote();
}

function submitTriviaVote() {
  if (selectedTriviaChoice === null) {
    console.log("submitTriviaVote: No choice selected, returning");
    return;
  }

  if (!requireConnection("triviaError")) {
    console.log("submitTriviaVote: Not connected, returning");
    return;
  }

  console.log("submitTriviaVote: Sending vote", {
    voter_token: voterToken,
    choice_index: selectedTriviaChoice,
  });

  // Send trivia vote
  const sent = wsConn.send({
    t: "submit_trivia_vote",
    voter_token: voterToken,
    choice_index: selectedTriviaChoice,
  });

  if (!sent) {
    showError("triviaError", "Verbindung verloren. Versuch's nochmal.");
    return;
  }

  hideError("triviaError");
}

function changeTriviaVote() {
  hasTriviaVoted = false;
  showTriviaScreen();
}

function updateTriviaVoteSummary() {
  const summaryEl = document.getElementById("triviaVoteSummary");
  if (!summaryEl || !triviaQuestion) return;

  if (
    selectedTriviaChoice !== null &&
    triviaQuestion.choices[selectedTriviaChoice]
  ) {
    const choiceText = triviaQuestion.choices[selectedTriviaChoice];
    const shortText =
      choiceText.length > 40 ? `${choiceText.substring(0, 40)}...` : choiceText;
    summaryEl.textContent = `${TRIVIA_LABELS[selectedTriviaChoice]}: "${shortText}"`;
  } else {
    summaryEl.textContent = "";
  }
}

function showTriviaResultScreen() {
  const questionEl = document.getElementById("triviaResultQuestionText");
  const container = document.getElementById("triviaResultOptions");
  const totalEl = document.getElementById("triviaResultTotal");

  if (!questionEl || !container || !triviaResult) return;

  // Update question text
  questionEl.textContent = triviaResult.question;

  // Render result choices (dynamic count)
  container.innerHTML = "";

  triviaResult.choices.forEach((choice, idx) => {
    const isCorrect = idx === triviaResult.correct_index;
    const voteCount = triviaResult.vote_counts[idx] || 0;

    const option = document.createElement("div");
    option.className = `trivia-result-option ${isCorrect ? "correct" : ""}`;
    option.innerHTML = `
      <span class="trivia-label">${TRIVIA_LABELS[idx]}</span>
      <span class="trivia-text">${escapeHtml(choice)}${isCorrect ? " ✓" : ""}</span>
      <span class="trivia-count">${voteCount}</span>
    `;
    container.appendChild(option);
  });

  // Update total
  if (totalEl) {
    totalEl.textContent = `Gesamt: ${triviaResult.total_votes} Stimmen`;
  }

  showScreen("triviaResultScreen");
}

if (typeof window !== "undefined") {
  Object.assign(window, {
    joinAudience,
    submitVote,
    togglePromptSubmission,
    submitPrompt,
    submitPromptVote,
    changePromptVote,
    changeTriviaVote,
  });
}

// Initialize on page load
init();
