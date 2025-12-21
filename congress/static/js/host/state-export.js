/**
 * Host panel state export/import functionality
 */

import { showAlert } from "./ui.js";

let cachedStateExport = null;

/**
 * Fetch and display the current state
 */
export async function refreshStateView() {
  const viewer = document.getElementById("stateJsonView");
  viewer.textContent = "Lade...";

  try {
    const response = await fetch("/api/state/export");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    cachedStateExport = await response.json();
    viewer.textContent = JSON.stringify(cachedStateExport, null, 2);
    showAlert("State geladen", "success");
  } catch (error) {
    viewer.textContent = `Fehler: ${error.message}`;
    showAlert(`Fehler beim Laden: ${error.message}`, "error");
  }
}

/**
 * Download the state as a JSON file
 */
export async function downloadStateExport() {
  try {
    // Always fetch fresh data for download
    const response = await fetch("/api/state/export");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    const data = await response.json();

    const blob = new Blob([JSON.stringify(data, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
    a.download = `gptdash-state-${timestamp}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    showAlert("State-Datei heruntergeladen", "success");
  } catch (error) {
    showAlert(`Fehler beim Download: ${error.message}`, "error");
  }
}

/**
 * Copy current state to clipboard
 */
export async function copyStateToClipboard() {
  try {
    const response = await fetch("/api/state/export");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    const data = await response.json();
    await navigator.clipboard.writeText(JSON.stringify(data, null, 2));
    showAlert("State in Zwischenablage kopiert", "success");
  } catch (error) {
    showAlert(`Fehler: ${error.message}`, "error");
  }
}

/**
 * Handle file selection for import
 */
export function handleStateFileSelect(event) {
  const file = event.target.files[0];
  if (!file) return;

  const reader = new FileReader();
  reader.onload = (e) => {
    document.getElementById("stateImportText").value = e.target.result;
    showImportStatus("Datei geladen. Klicke 'Validieren' zur Prfung.", "info");
  };
  reader.onerror = () => {
    showImportStatus("Fehler beim Lesen der Datei", "error");
  };
  reader.readAsText(file);
}

/**
 * Validate the import JSON without importing
 */
export function validateStateImport() {
  const text = document.getElementById("stateImportText").value.trim();
  if (!text) {
    showImportStatus("Bitte JSON eingeben oder Datei hochladen", "error");
    return null;
  }

  try {
    const data = JSON.parse(text);

    // Basic validation
    if (!data.schema_version) {
      showImportStatus("Fehler: schema_version fehlt", "error");
      return null;
    }

    // Count objects for summary
    const summary = [];
    if (data.game) summary.push("1 Game");
    if (data.rounds) summary.push(`${Object.keys(data.rounds).length} Runden`);
    if (data.players)
      summary.push(`${Object.keys(data.players).length} Spieler`);
    if (data.submissions)
      summary.push(`${Object.keys(data.submissions).length} Antworten`);
    if (data.votes) summary.push(`${Object.keys(data.votes).length} Votes`);
    if (data.scores) summary.push(`${data.scores.length} Scores`);

    showImportStatus(
      ` JSON valide (Schema v${data.schema_version}): ${summary.join(", ")}`,
      "success",
    );
    return data;
  } catch (error) {
    showImportStatus(`JSON-Parsing Fehler: ${error.message}`, "error");
    return null;
  }
}

/**
 * Execute the state import
 */
export async function executeStateImport() {
  const data = validateStateImport();
  if (!data) return;

  if (
    !confirm(
      "ACHTUNG: Der gesamte Spielzustand wird ersetzt!\n\n" +
        "Alle verbundenen Clients werden ber den neuen Zustand informiert.\n\n" +
        "Fortfahren?",
    )
  ) {
    return;
  }

  showImportStatus("Importiere...", "info");

  try {
    const response = await fetch("/api/state/import", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(errorText);
    }

    showImportStatus(" State erfolgreich importiert!", "success");
    showAlert("State importiert! UI wird aktualisiert...", "success");

    // Clear the import textarea
    document.getElementById("stateImportText").value = "";
    document.getElementById("stateImportFile").value = "";

    // Refresh the state view
    await refreshStateView();
  } catch (error) {
    showImportStatus(`Import fehlgeschlagen: ${error.message}`, "error");
    showAlert(`Import fehlgeschlagen: ${error.message}`, "error");
  }
}

/**
 * Show status message in import section
 */
function showImportStatus(message, type) {
  const el = document.getElementById("stateImportStatus");
  if (!el) return;
  el.innerHTML = "";
  const alertEl = document.createElement("div");
  alertEl.className = `alert ${type}`;
  alertEl.style.margin = "0";
  alertEl.textContent = message;
  el.appendChild(alertEl);
}
