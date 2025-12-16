/**
 * Host panel submissions management and drag-drop reveal ordering
 */

import { gameState, dragState } from "./state.js";
import { escapeHtml } from "../common.js";
import { showAlert } from "./ui.js";
import { updateAiSubmissionsList } from "./ai-manager.js";

// WebSocket connection reference (set via setWsConn)
let wsConn = null;

export function setWsConn(conn) {
  wsConn = conn;
}

/**
 * Update submissions list with drag-drop functionality
 */
export function updateSubmissionsList() {
  const container = document.getElementById("submissionsList");
  if (!container) return;
  container.innerHTML = "";

  // Also update the AI-specific submissions list
  updateAiSubmissionsList();

  if (gameState.submissions.length === 0) {
    container.innerHTML = '<p style="opacity: 0.6;">Noch keine Antworten</p>';
    gameState.revealOrder = [];
    return;
  }

  // Build reveal order: use existing order, add new submissions at end
  const existingIds = new Set(gameState.revealOrder);
  const currentIds = new Set(gameState.submissions.map((s) => s.id));

  // Remove IDs that no longer exist
  gameState.revealOrder = gameState.revealOrder.filter((id) =>
    currentIds.has(id),
  );

  // Add new submissions at the end
  gameState.submissions.forEach((sub) => {
    if (!existingIds.has(sub.id)) {
      gameState.revealOrder.push(sub.id);
    }
  });

  // Create a map for quick submission lookup
  const submissionMap = new Map(gameState.submissions.map((s) => [s.id, s]));

  // Render submissions in reveal order
  gameState.revealOrder.forEach((subId, index) => {
    const sub = submissionMap.get(subId);
    if (!sub) return;

    const div = document.createElement("div");
    const authorKind = sub.author_kind || "unknown";
    const isSelectedAi =
      authorKind === "ai" && gameState.selectedAiSubmissionId === sub.id;
    div.className = `submission-card draggable${authorKind === "ai" ? " ai" : ""}${isSelectedAi ? " selected-ai" : ""}`;
    div.dataset.submissionId = sub.id;
    div.draggable = true;

    // Show provider info for AI submissions
    const providerInfo =
      authorKind === "ai" && sub.author_ref
        ? `<span class="provider-info">(${escapeHtml(sub.author_ref)})</span>`
        : "";

    div.innerHTML = `
      <div class="header">
        <div class="drag-handle">
          <span class="order-number">${index + 1}</span>
          <span class="drag-icon"></span>
        </div>
        <div class="header-content">
          <span style="font-size: 12px; opacity: 0.6;">${sub.id.substring(0, 12)}...</span>
          <div>
            <span class="badge ${authorKind}">${authorKind.toUpperCase()}</span>
            ${providerInfo}
            ${isSelectedAi ? '<span class="badge selected">AUSGEWHLT</span>' : ""}
          </div>
        </div>
      </div>
      <div class="text">${escapeHtml(sub.display_text)}</div>
      <div class="actions">
        ${authorKind === "ai" && !isSelectedAi ? `<button data-action="select-ai" data-submission-id="${escapeHtml(sub.id)}">Als KI auswhlen</button>` : ""}
        ${authorKind === "ai" ? `<button class="remove-btn" data-action="remove-submission" data-submission-id="${escapeHtml(sub.id)}">Entfernen</button>` : ""}
        ${authorKind === "player" ? `<button class="danger" data-action="mark-duplicate" data-submission-id="${escapeHtml(sub.id)}">Dupe</button>` : ""}
        <button class="secondary" data-action="edit-submission" data-submission-id="${escapeHtml(sub.id)}">Bearbeiten</button>
      </div>
    `;

    // Add drag event listeners
    div.addEventListener("dragstart", handleDragStart);
    div.addEventListener("dragend", handleDragEnd);
    div.addEventListener("dragover", handleDragOver);
    div.addEventListener("dragenter", handleDragEnter);
    div.addEventListener("dragleave", handleDragLeave);
    div.addEventListener("drop", handleDrop);

    container.appendChild(div);
  });

  // Update the manual input field with current order
  const revealOrderInput = document.getElementById("revealOrderInput");
  if (revealOrderInput) {
    revealOrderInput.value = gameState.revealOrder.join(", ");
  }
}

// Drag-and-drop handlers
function handleDragStart(e) {
  const card = e.target.closest(".submission-card");
  if (!card) return;

  dragState.draggedId = card.dataset.submissionId;
  dragState.draggedElement = card;
  card.classList.add("dragging");

  e.dataTransfer.effectAllowed = "move";
  e.dataTransfer.setData("text/plain", dragState.draggedId);
}

function handleDragEnd(e) {
  const card = e.target.closest(".submission-card");
  if (card) {
    card.classList.remove("dragging");
  }

  // Remove all drag-over indicators
  document.querySelectorAll(".submission-card.drag-over").forEach((el) => {
    el.classList.remove("drag-over");
  });

  dragState.draggedId = null;
  dragState.draggedElement = null;
}

function handleDragOver(e) {
  e.preventDefault();
  e.dataTransfer.dropEffect = "move";
}

function handleDragEnter(e) {
  e.preventDefault();
  const card = e.target.closest(".submission-card");
  if (card && card !== dragState.draggedElement) {
    card.classList.add("drag-over");
  }
}

function handleDragLeave(e) {
  const card = e.target.closest(".submission-card");
  if (card && !card.contains(e.relatedTarget)) {
    card.classList.remove("drag-over");
  }
}

function handleDrop(e) {
  e.preventDefault();
  const targetCard = e.target.closest(".submission-card");
  if (!targetCard || !dragState.draggedId) return;

  const targetId = targetCard.dataset.submissionId;
  if (targetId === dragState.draggedId) return;

  targetCard.classList.remove("drag-over");

  // Reorder the reveal order array
  const fromIndex = gameState.revealOrder.indexOf(dragState.draggedId);
  const toIndex = gameState.revealOrder.indexOf(targetId);

  if (fromIndex === -1 || toIndex === -1) return;

  // Remove from old position and insert at new position
  gameState.revealOrder.splice(fromIndex, 1);
  gameState.revealOrder.splice(toIndex, 0, dragState.draggedId);

  // Re-render the list
  updateSubmissionsList();

  // Send the new order to the server
  sendRevealOrder();
}

/**
 * Send the current reveal order to the server
 */
export function sendRevealOrder() {
  if (gameState.revealOrder.length === 0) return;
  if (!wsConn) return;

  wsConn.send({
    t: "host_set_reveal_order",
    order: gameState.revealOrder,
  });

  showAlert("Reihenfolge aktualisiert", "success");
}

/**
 * Edit a submission's display text
 */
export function editSubmission(submissionId, conn) {
  const wsConnection = conn || wsConn;
  const submission = gameState.submissions.find((s) => s.id === submissionId);
  if (!submission) {
    showAlert("Antwort nicht gefunden", "error");
    return;
  }

  const newText = prompt("Antwort bearbeiten:", submission.display_text);

  if (newText === null) {
    return; // User cancelled
  }

  if (newText.trim() === "") {
    showAlert("Antwort darf nicht leer sein", "error");
    return;
  }

  wsConnection.send({
    t: "host_edit_submission",
    submission_id: submissionId,
    new_text: newText,
  });
  showAlert("Antwort aktualisiert", "success");
}

/**
 * Mark a submission as duplicate
 */
export function markDuplicate(submissionId, conn) {
  const wsConnection = conn || wsConn;
  if (
    confirm(
      "Diese Antwort als Duplikat markieren?\n\nDer Spieler wird benachrichtigt und muss eine neue Antwort einreichen.",
    )
  ) {
    wsConnection.send({
      t: "host_mark_duplicate",
      submission_id: submissionId,
    });
    showAlert("Antwort als Duplikat markiert", "success");
  }
}

/**
 * Set reveal order from manual input
 */
export function setRevealOrder(conn) {
  const wsConnection = conn || wsConn;
  const input = document.getElementById("revealOrderInput").value.trim();
  if (!input) {
    alert("Bitte gib Antwort-IDs ein");
    return;
  }

  const order = input.split(",").map((s) => s.trim());

  wsConnection.send({
    t: "host_set_reveal_order",
    order: order,
  });
}
