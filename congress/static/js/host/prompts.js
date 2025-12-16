/**
 * Host panel prompt management
 */

import { gameState, promptSectionState } from "./state.js";
import { escapeHtml, debounce } from "../common.js";
import { showAlert } from "./ui.js";

// State for auto-queueing prompts after adding from overview
export let pendingOverviewPromptAutoQueue = null;

export function setPendingOverviewPromptAutoQueue(value) {
  pendingOverviewPromptAutoQueue = value;
}

/**
 * Add a prompt from the prompts panel
 */
export function addPrompt(wsConn) {
  const text = document.getElementById("promptText").value.trim();
  const imageUrl =
    document.getElementById("promptImageUrl")?.value.trim() || null;

  // Require at least text or image
  if (!text && !imageUrl) {
    alert("Bitte gib einen Prompt-Text oder eine Bild-URL ein");
    return;
  }

  wsConn.send({
    t: "host_add_prompt",
    text: text || null,
    image_url: imageUrl || null,
  });

  document.getElementById("promptText").value = "";
  if (document.getElementById("promptImageUrl")) {
    document.getElementById("promptImageUrl").value = "";
  }
  // Clear image preview
  const preview = document.getElementById("promptImagePreview");
  if (preview) {
    preview.innerHTML = "";
  }
}

/**
 * Add a prompt from the overview panel
 */
export function addPromptFromOverview(queueAfterAdd, wsConn) {
  const text =
    document.getElementById("overviewPromptText")?.value.trim() ?? "";
  const imageUrl =
    document.getElementById("overviewPromptImageUrl")?.value.trim() || null;

  // Require at least text or image
  if (!text && !imageUrl) {
    alert("Bitte gib einen Prompt-Text oder eine Bild-URL ein");
    return;
  }

  if (queueAfterAdd) {
    pendingOverviewPromptAutoQueue = {
      text: text || null,
      image_url: imageUrl || null,
    };
  }

  wsConn.send({
    t: "host_add_prompt",
    text: text || null,
    image_url: imageUrl || null,
  });

  const textEl = document.getElementById("overviewPromptText");
  if (textEl) textEl.value = "";
  const imageEl = document.getElementById("overviewPromptImageUrl");
  if (imageEl) imageEl.value = "";
  const preview = document.getElementById("overviewPromptImagePreview");
  if (preview) preview.innerHTML = "";
}

/**
 * Setup image preview for prompt inputs
 */
export function setupImagePreview() {
  setupImagePreviewFor("promptImageUrl", "promptImagePreview");
  setupImagePreviewFor("overviewPromptImageUrl", "overviewPromptImagePreview");
}

function setupImagePreviewFor(inputId, previewId) {
  const imageUrlInput = document.getElementById(inputId);
  if (!imageUrlInput) return;

  imageUrlInput.addEventListener(
    "input",
    debounce((e) => {
      const url = e.target.value.trim();
      const preview = document.getElementById(previewId);
      if (!preview) return;

      if (!url) {
        preview.innerHTML = "";
        return;
      }

      // Show loading state
      preview.innerHTML = '<p style="opacity: 0.6;">Lade Vorschau...</p>';

      // Create image element to test URL
      const img = new Image();
      img.onload = () => {
        preview.innerHTML = `<img src="${escapeHtml(url)}" style="max-width: 300px; max-height: 200px; border-radius: 8px; border: 1px solid rgba(255,255,255,0.2);" alt="Prompt-Bild Vorschau">`;
      };
      img.onerror = () => {
        preview.innerHTML =
          '<p style="color: #ff6b6b;">Bild konnte nicht geladen werden</p>';
      };
      img.src = url;
    }, 500),
  );
}

/**
 * Legacy prompt selection via ID input
 */
export function selectPrompt(wsConn) {
  const promptId = prompt("Gib die Prompt-ID ein:");
  if (promptId) {
    wsConn.send({
      t: "host_select_prompt",
      prompt_id: promptId,
    });
  }
}

/**
 * Select a prompt by ID
 */
export function selectPromptById(promptId, wsConn) {
  wsConn.send({
    t: "host_select_prompt",
    prompt_id: promptId,
  });
}

/**
 * Update prompt statistics display
 */
export function updatePromptStats() {
  const stats = gameState.promptStats;

  // Update stat values
  document.getElementById("promptStatTotal").textContent = stats.total || 0;
  document.getElementById("promptStatHost").textContent = stats.host_count || 0;
  document.getElementById("promptStatAudience").textContent =
    stats.audience_count || 0;

  // Update top submitters section
  const topSection = document.getElementById("topSubmittersSection");
  const topList = document.getElementById("topSubmittersList");

  if (stats.top_submitters && stats.top_submitters.length > 0) {
    topSection.style.display = "block";
    topList.innerHTML = stats.top_submitters
      .map((s) => {
        const shortId = s.voter_id.substring(0, 8);
        return `
        <div class="top-submitter-item">
          <span class="submitter-id" title="${escapeHtml(s.voter_id)}">${shortId}...</span>
          <span class="submitter-count">${s.count} Prompts</span>
          <button class="danger small" onclick="shadowbanAudience('${s.voter_id}')" title="Shadowban"></button>
        </div>
      `;
      })
      .join("");
  } else {
    topSection.style.display = "none";
  }
}

/**
 * Toggle collapsible prompt section
 */
export function togglePromptSection(sectionKey) {
  const isExpanded = promptSectionState[sectionKey];
  promptSectionState[sectionKey] = !isExpanded;

  // Persist state
  localStorage.setItem(
    `promptSection_${sectionKey}`,
    isExpanded ? "collapsed" : "expanded",
  );

  // Update UI
  const listId =
    sectionKey === "hostPrompts" ? "hostPromptsList" : "audiencePromptsList";
  const toggleId =
    sectionKey === "hostPrompts"
      ? "hostPromptsToggle"
      : "audiencePromptsToggle";

  const list = document.getElementById(listId);
  const toggle = document.getElementById(toggleId);

  if (list) {
    list.classList.toggle("collapsed", isExpanded);
  }
  if (toggle) {
    toggle.textContent = isExpanded ? "" : "";
  }
}

/**
 * Filter prompts by search query
 */
export function filterPrompts() {
  const query = document
    .getElementById("promptSearchInput")
    .value.toLowerCase()
    .trim();

  // Filter all prompt rows
  document.querySelectorAll(".prompt-row").forEach((row) => {
    const text = row.dataset.searchText || "";
    const matches = !query || text.toLowerCase().includes(query);
    row.style.display = matches ? "" : "none";
  });
}

/**
 * Pick a random prompt from available prompts and add to queue
 */
export function pickRandomPrompt(wsConn) {
  const queuedIds = new Set(gameState.queuedPrompts.map((p) => p.id));
  const availablePrompts = gameState.prompts.filter(
    (p) => !queuedIds.has(p.id),
  );

  if (availablePrompts.length === 0) {
    showAlert("Keine verfgbaren Prompts zum Auswhlen", "warning");
    return;
  }

  if (gameState.queuedPrompts.length >= 3) {
    showAlert("Warteschlange ist voll (max. 3)", "warning");
    return;
  }

  const randomPrompt =
    availablePrompts[Math.floor(Math.random() * availablePrompts.length)];
  queuePrompt(randomPrompt.id, wsConn);
}

/**
 * Shadowban all submitters of a deduplicated prompt
 */
export function shadowbanPromptSubmitters(promptId, wsConn) {
  const foundPrompt = gameState.prompts.find((p) => p.id === promptId);
  if (!foundPrompt) return;

  const submitterCount = foundPrompt.submitter_ids?.length || 0;
  const message =
    submitterCount > 1
      ? `Alle ${submitterCount} Einreicher dieses Prompts shadowbannen?\n\nDiese Nutzer knnen weiterhin Prompts einreichen, aber sie werden ignoriert.`
      : "Diesen Einreicher shadowbannen?\n\nDer Nutzer kann weiterhin Prompts einreichen, aber sie werden ignoriert.";

  if (confirm(message)) {
    wsConn.send({
      t: "host_shadowban_prompt_submitters",
      prompt_id: promptId,
    });
    showAlert(`${submitterCount} Nutzer shadowbanned`, "success");
  }
}

/**
 * Shadowban a specific audience member
 */
export function shadowbanAudience(voterId, wsConn) {
  if (
    confirm(
      "Diesen Nutzer shadowbannen?\n\nAlle zuknftigen Prompts von diesem Nutzer werden ignoriert. Der Nutzer erfhrt davon nichts.",
    )
  ) {
    wsConn.send({
      t: "host_shadowban_audience",
      voter_id: voterId,
    });
    showAlert("Nutzer shadowbanned", "success");
  }
}

/**
 * Render a single compact prompt row
 */
export function renderPromptRow(promptData, queuedIds, queueFull) {
  const isQueued = queuedIds.has(promptData.id);
  const hasImage = !!promptData.image_url;
  const hasText = !!promptData.text;
  const submitterCount = promptData.submission_count || 1;

  // Build display text
  let displayText = "";
  if (hasText) {
    displayText = promptData.text;
  } else if (hasImage) {
    displayText = "[Bild-Prompt]";
  } else {
    displayText = "(Leerer Prompt)";
  }

  // Truncate for display
  const truncatedText =
    displayText.length > 60
      ? `${displayText.substring(0, 60)}...`
      : displayText;

  // Build tooltip content
  const escapedTooltipText = escapeHtml(displayText);
  let tooltipContent = escapedTooltipText;
  if (hasImage) {
    tooltipContent = `<img src="${escapeHtml(promptData.image_url)}" alt="Prompt-Bild" style="max-width: 300px; max-height: 200px;"><br>${escapedTooltipText}`;
  }

  // Submitter info for audience prompts
  const submitterInfo =
    promptData.source === "audience" && promptData.submitter_ids?.length > 0
      ? `<span class="submitter-count" title="${promptData.submitter_ids.length} Einreicher">${submitterCount}x</span>`
      : "";

  const div = document.createElement("div");
  div.className = `prompt-row${isQueued ? " queued" : ""}`;
  div.dataset.searchText = displayText;
  div.dataset.promptId = promptData.id;

  div.innerHTML = `
    <div class="prompt-row-content">
      ${hasImage ? '<span class="prompt-image-indicator" title="Hat Bild"></span>' : ""}
      <span class="prompt-text-preview" title="${escapeHtml(displayText)}">${escapeHtml(truncatedText)}</span>
      ${submitterInfo}
    </div>
    <div class="prompt-row-actions">
      ${
        isQueued
          ? '<span class="queued-badge"></span>'
          : queueFull
            ? ""
            : `<button class="action-btn queue-btn" onclick="event.stopPropagation(); queuePrompt('${promptData.id}')" title="In Warteschlange">+</button>`
      }
      ${
        promptData.source === "audience" && promptData.submitter_ids?.length > 0
          ? `<button class="action-btn ban-btn" onclick="event.stopPropagation(); shadowbanPromptSubmitters('${promptData.id}')" title="Einreicher shadowbannen"></button>`
          : ""
      }
      <button class="action-btn delete-btn" onclick="event.stopPropagation(); deletePrompt('${promptData.id}')" title="Lschen"></button>
    </div>
    <div class="prompt-tooltip">${tooltipContent}</div>
  `;

  return div;
}

/**
 * Update the prompts list display
 */
export function updatePromptsList() {
  // Update stats first
  updatePromptStats();

  const hostList = document.getElementById("hostPromptsList");
  const audienceList = document.getElementById("audiencePromptsList");

  if (!hostList || !audienceList) return;

  // Get queued prompt IDs
  const queuedIds = new Set(gameState.queuedPrompts.map((p) => p.id));
  const queueFull = gameState.queuedPrompts.length >= 3;

  // Separate prompts by source
  const hostPrompts = gameState.prompts.filter((p) => p.source === "host");
  const audiencePrompts = gameState.prompts.filter(
    (p) => p.source === "audience",
  );

  // Update counts
  document.getElementById("hostPromptsCount").textContent = hostPrompts.length;
  document.getElementById("audiencePromptsCount").textContent =
    audiencePrompts.length;

  // Render host prompts
  hostList.innerHTML = "";
  if (hostPrompts.length === 0) {
    hostList.innerHTML = '<p class="no-prompts-hint">Keine Host-Prompts</p>';
  } else {
    hostPrompts.forEach((p) => {
      hostList.appendChild(renderPromptRow(p, queuedIds, queueFull));
    });
  }

  // Render audience prompts
  audienceList.innerHTML = "";
  if (audiencePrompts.length === 0) {
    audienceList.innerHTML =
      '<p class="no-prompts-hint">Keine Publikums-Prompts</p>';
  } else {
    audiencePrompts.forEach((p) => {
      audienceList.appendChild(renderPromptRow(p, queuedIds, queueFull));
    });
  }

  // Apply current collapse state
  const hostToggle = document.getElementById("hostPromptsToggle");
  const audienceToggle = document.getElementById("audiencePromptsToggle");

  if (!promptSectionState.hostPrompts) {
    hostList.classList.add("collapsed");
    if (hostToggle) hostToggle.textContent = "";
  }
  if (!promptSectionState.audiencePrompts) {
    audienceList.classList.add("collapsed");
    if (audienceToggle) audienceToggle.textContent = "";
  }

  // Apply search filter if there's a query
  const searchInput = document.getElementById("promptSearchInput");
  if (searchInput?.value.trim()) {
    filterPrompts();
  }
}

/**
 * Update queued prompts list in both containers
 */
export function updateQueuedPromptsList() {
  updateQueuedPromptsListInto("queuedPromptsList", "startPromptSelectionBtn");
  updateQueuedPromptsListInto(
    "overviewQueuedPromptsList",
    "overviewStartPromptSelectionBtn",
  );
}

function updateQueuedPromptsListInto(containerId, startBtnId) {
  const container = document.getElementById(containerId);
  if (!container) return;
  container.innerHTML = "";

  // Always update start button visibility first
  const startBtn = document.getElementById(startBtnId);
  if (startBtn) {
    startBtn.style.display =
      gameState.queuedPrompts.length > 0 ? "block" : "none";
    const count = gameState.queuedPrompts.length;
    if (count === 1) {
      startBtn.textContent = " Runde starten (1 Prompt  direkt zu Schreiben)";
    } else if (count > 1) {
      startBtn.textContent = ` Prompt-Voting starten (${count} Prompts)`;
    }
  }

  if (gameState.queuedPrompts.length === 0) {
    container.innerHTML =
      '<p style="opacity: 0.6;">Keine Prompts in der Warteschlange. Whle Prompts aus dem Pool.</p>';
    return;
  }

  gameState.queuedPrompts.forEach((promptData, idx) => {
    const div = document.createElement("div");
    div.className = "prompt-card queued";

    const hasImage = !!promptData.image_url;
    const hasText = !!promptData.text;

    let contentHtml = "";
    if (hasImage) {
      contentHtml += `<div class="prompt-image"><img src="${escapeHtml(promptData.image_url)}" alt="Prompt-Bild" style="max-width: 150px; max-height: 100px; border-radius: 4px;"></div>`;
    }
    if (hasText) {
      contentHtml += `<div class="prompt-text">${escapeHtml(promptData.text)}</div>`;
    }

    div.innerHTML = `
      <div class="prompt-header">
        <span class="queue-number">#${idx + 1}</span>
        ${hasImage ? '<span class="badge multimodal">Bild</span>' : ""}
      </div>
      ${contentHtml}
      <div class="prompt-actions">
        <button class="secondary small" onclick="unqueuePrompt('${promptData.id}')" title="Zurck in den Pool"> Pool</button>
        <button class="danger small" onclick="deletePrompt('${promptData.id}')" title="Prompt lschen"></button>
      </div>
    `;
    container.appendChild(div);
  });
}

/**
 * Queue a prompt for the next round
 */
export function queuePrompt(promptId, wsConn) {
  wsConn.send({
    t: "host_queue_prompt",
    prompt_id: promptId,
  });
}

/**
 * Remove a prompt from the queue
 */
export function unqueuePrompt(promptId, wsConn) {
  wsConn.send({
    t: "host_unqueue_prompt",
    prompt_id: promptId,
  });
}

/**
 * Delete a prompt from the pool
 */
export function deletePrompt(promptId, wsConn) {
  wsConn.send({
    t: "host_delete_prompt",
    prompt_id: promptId,
  });
}

/**
 * Start prompt selection phase
 */
export function startPromptSelection(wsConn) {
  if (gameState.queuedPrompts.length === 0) {
    showAlert("Keine Prompts in der Warteschlange", "warning");
    return;
  }
  wsConn.send({
    t: "host_transition_phase",
    phase: "PROMPT_SELECTION",
  });
}

/**
 * Try to auto-queue a prompt that was just added from overview
 */
export function maybeAutoQueueOverviewPrompt(wsConn) {
  if (!pendingOverviewPromptAutoQueue) return;

  const queuedIds = new Set(gameState.queuedPrompts.map((p) => p.id));
  const { text, image_url } = pendingOverviewPromptAutoQueue;

  const candidates = (gameState.prompts || [])
    .filter((p) => p.source === "host")
    .filter((p) => !queuedIds.has(p.id))
    .filter((p) => (p.text ?? null) === (text ?? null))
    .filter((p) => (p.image_url ?? null) === (image_url ?? null))
    .sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""));

  if (candidates.length === 0) return;

  queuePrompt(candidates[0].id, wsConn);
  pendingOverviewPromptAutoQueue = null;
}
