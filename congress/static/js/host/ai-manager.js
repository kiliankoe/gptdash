/**
 * Host panel AI submission management
 */

import { gameState } from "./state.js";
import { escapeHtml } from "../common.js";
import { showAlert } from "./ui.js";

/**
 * Fetch available AI models from the server
 */
export async function fetchAvailableModels() {
  try {
    const response = await fetch("/api/models");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const data = await response.json();
    gameState.availableModels = data;

    // Set default model if not already set
    if (gameState.selectedModel === null && data.default_model) {
      gameState.selectedModel = data.default_model;
    }

    updateModelSelectorUI();
    console.log("Available models loaded:", data);
  } catch (e) {
    console.error("Failed to fetch available models:", e);
    showAlert("Konnte KI-Modelle nicht laden", "warning");
  }
}

/**
 * Update the model selector dropdown UI
 */
export function updateModelSelectorUI() {
  const selector = document.getElementById("modelSelector");
  if (!selector) return;

  // Clear and add default option
  selector.innerHTML = '<option value="">Alle Provider (Standard)</option>';

  const { openai_models, ollama_models } = gameState.availableModels;

  // Add OpenAI models
  if (openai_models && openai_models.length > 0) {
    const group = document.createElement("optgroup");
    group.label = "OpenAI";
    openai_models.forEach((m) => {
      const opt = document.createElement("option");
      opt.value = m.id;
      opt.textContent = m.name + (m.supports_vision ? " (Vision)" : "");
      if (m.id === gameState.selectedModel) opt.selected = true;
      group.appendChild(opt);
    });
    selector.appendChild(group);
  }

  // Add Ollama models
  if (ollama_models && ollama_models.length > 0) {
    const group = document.createElement("optgroup");
    group.label = "Ollama (lokal)";
    ollama_models.forEach((m) => {
      const opt = document.createElement("option");
      opt.value = m.id;
      opt.textContent = m.name + (m.supports_vision ? " (Vision)" : "");
      if (m.id === gameState.selectedModel) opt.selected = true;
      group.appendChild(opt);
    });
    selector.appendChild(group);
  }
}

/**
 * Get currently selected model ID
 * @returns {string|null} Model ID or null for all providers
 */
export function getSelectedModel() {
  return gameState.selectedModel || null;
}

/**
 * Set selected model
 * @param {string} modelId - Model ID or empty string for all providers
 */
export function setSelectedModel(modelId) {
  gameState.selectedModel = modelId || null;
  console.log("Selected model:", gameState.selectedModel || "All providers");
}

/**
 * Regenerate AI submissions
 */
export function regenerateAi(wsConn) {
  const model = getSelectedModel();
  wsConn.send({
    t: "host_regenerate_ai",
    model: model,
  });
  showAlert(
    model
      ? `KI-Generierung gestartet (${model})...`
      : "KI-Generierung gestartet...",
    "info",
  );
}

/**
 * Write a manual AI submission
 */
export function writeManualAiSubmission(wsConn) {
  const text = document.getElementById("manualAiText").value.trim();
  if (!text) {
    showAlert("Bitte gib einen Text fr die KI-Antwort ein", "error");
    return;
  }
  wsConn.send({
    t: "host_write_ai_submission",
    text: text,
  });
  document.getElementById("manualAiText").value = "";
  showAlert("Manuelle KI-Antwort gespeichert", "success");
}

/**
 * Select an AI submission as the one to use
 */
export function selectAiSubmission(submissionId, wsConn) {
  wsConn.send({
    t: "host_set_ai_submission",
    submission_id: submissionId,
  });
  gameState.selectedAiSubmissionId = submissionId;
  updateAiSubmissionsList();
  showAlert("KI-Antwort ausgewhlt", "success");
}

/**
 * Remove a submission
 */
export function removeSubmission(submissionId, wsConn) {
  const sub = gameState.submissions.find((s) => s.id === submissionId);
  const label =
    sub?.author_kind === "ai"
      ? "Diese KI-Antwort entfernen?"
      : "Diese Antwort entfernen?";

  if (!confirm(`${label}\n\nDas kann nicht rckgngig gemacht werden.`)) {
    return;
  }

  wsConn.send({
    t: "host_remove_submission",
    submission_id: submissionId,
  });

  if (gameState.selectedAiSubmissionId === submissionId) {
    gameState.selectedAiSubmissionId = null;
  }

  showAlert("Antwort entfernt", "success");
}

/**
 * Handle AI generation status message from server
 */
export function handleAiGenerationStatus(message) {
  gameState.aiGenerationStatus = message.status;
  updateAiGenerationStatusUI(message);

  switch (message.status) {
    case "started":
      showAlert("KI-Generierung gestartet...", "info");
      break;
    case "completed":
      showAlert("KI-Generierung abgeschlossen!", "success");
      break;
    case "all_failed":
      showAlert(
        `KI-Generierung fehlgeschlagen: ${message.message || "Unbekannter Fehler"}`,
        "error",
      );
      break;
  }
}

/**
 * Update AI generation status UI
 */
function updateAiGenerationStatusUI(message) {
  const statusEl = document.getElementById("aiGenerationStatus");
  if (!statusEl) return;

  let statusText = "";
  let statusClass = "";

  switch (message?.status || gameState.aiGenerationStatus) {
    case "started":
      statusText = "KI-Status: Generiere Antworten...";
      statusClass = "info";
      break;
    case "completed":
      statusText = "KI-Status: Generierung abgeschlossen";
      statusClass = "success";
      break;
    case "all_failed":
      statusText = `KI-Status: Fehlgeschlagen - ${message?.message || "Alle Provider haben keine Antwort geliefert"}`;
      statusClass = "error";
      break;
    default:
      statusText = "KI-Status: Warte auf Prompt-Auswahl";
      statusClass = "";
  }

  statusEl.innerHTML = "";
  const p = document.createElement("p");
  p.className = statusClass;
  p.style.margin = "0";
  p.textContent = statusText;
  statusEl.appendChild(p);
}

/**
 * Update AI submissions list display
 */
export function updateAiSubmissionsList() {
  const container = document.getElementById("aiSubmissionsList");
  if (!container) return;

  const aiSubmissions = gameState.submissions.filter(
    (s) => s.author_kind === "ai",
  );

  if (
    gameState.selectedAiSubmissionId &&
    !aiSubmissions.some((s) => s.id === gameState.selectedAiSubmissionId)
  ) {
    gameState.selectedAiSubmissionId = null;
  }

  if (aiSubmissions.length === 0) {
    container.innerHTML =
      '<p style="opacity: 0.6;">Keine KI-Antworten vorhanden</p>';
    return;
  }

  let html =
    '<p style="margin-bottom: 10px; opacity: 0.8;">Whle die KI-Antwort fr diese Runde:</p>';
  html += '<div class="ai-submissions-grid">';

  aiSubmissions.forEach((sub) => {
    const isSelected = gameState.selectedAiSubmissionId === sub.id;
    const provider = sub.author_ref || "unbekannt";
    // Parse provider info (format: "provider:model")
    const providerParts = provider.split(":");
    const providerName = providerParts[0] || "?";
    const modelName = providerParts[1] || "";

    html += `
      <div class="ai-submission-card ${isSelected ? "selected" : ""}" data-action="select-ai" data-submission-id="${escapeHtml(sub.id)}">
        <div class="ai-card-header">
          <span class="provider-badge">${escapeHtml(providerName)}</span>
          ${modelName ? `<span class="model-name">${escapeHtml(modelName)}</span>` : ""}
          ${isSelected ? '<span class="selected-badge">AUSGEWHLT</span>' : ""}
        </div>
        <div class="ai-card-text">${escapeHtml(sub.display_text)}</div>
        <div class="ai-card-actions">
          <button class="${isSelected ? "" : "secondary"}" data-action="select-ai" data-submission-id="${escapeHtml(sub.id)}">
            ${isSelected ? "Ausgewhlt" : "Auswhlen"}
          </button>
          <button class="remove-btn" data-action="remove-submission" data-submission-id="${escapeHtml(sub.id)}">
            Entfernen
          </button>
        </div>
      </div>
    `;
  });

  html += "</div>";
  container.innerHTML = html;
}
