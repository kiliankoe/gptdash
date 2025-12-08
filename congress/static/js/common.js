/**
 * Common JavaScript utilities for GPTDash clients
 */

/**
 * WebSocket connection manager
 */
class WSConnection {
  constructor(role, onMessage, onStatusChange, token = null) {
    this.role = role;
    this.onMessage = onMessage;
    this.onStatusChange = onStatusChange;
    this.token = token;
    this.ws = null;
    this.reconnectDelay = 2000;
  }

  setToken(token) {
    this.token = token;
  }

  connect() {
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const host = window.location.host || window.location.hostname;
    let wsUrl = `${protocol}://${host}/ws?role=${this.role}`;
    if (this.token) {
      wsUrl += `&token=${encodeURIComponent(this.token)}`;
    }
    console.log("Connecting to:", wsUrl);

    this.ws = new WebSocket(wsUrl);

    this.ws.onopen = () => {
      console.log("Connected to game server");
      if (this.onStatusChange) {
        this.onStatusChange(true, "Connected");
      }
    };

    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      console.log("Received:", message);
      if (this.onMessage) {
        this.onMessage(message);
      }
    };

    this.ws.onerror = (error) => {
      console.error("WebSocket error:", error);
      if (this.onStatusChange) {
        this.onStatusChange(false, "Connection error");
      }
    };

    this.ws.onclose = () => {
      console.log("Disconnected from game server");
      if (this.onStatusChange) {
        this.onStatusChange(false, "disconnected");
      }
      // Auto-reconnect
      setTimeout(() => this.connect(), this.reconnectDelay);
    };
  }

  send(message) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
      console.log("Sent:", message);
      return true;
    } else {
      console.error("WebSocket not connected");
      return false;
    }
  }

  isConnected() {
    return this.ws && this.ws.readyState === WebSocket.OPEN;
  }
}

/**
 * Update connection status UI
 */
function updateConnectionStatus(connected, text) {
  const statusEl = document.getElementById("connectionStatus");
  if (statusEl) {
    statusEl.className = `connection-status ${connected ? "connected" : "disconnected"}`;
    statusEl.textContent =
      text || (connected ? "Verbunden" : "Nicht verbunden");
  }

  const dotEl = document.getElementById("statusDot");
  if (dotEl) {
    if (connected) {
      dotEl.classList.add("connected");
    } else {
      dotEl.classList.remove("connected");
    }
  }

  const textEl = document.getElementById("statusText");
  if (textEl) {
    textEl.textContent = text || (connected ? "Verbunden" : "Nicht verbunden");
  }
}

/**
 * Show/hide screens
 */
function showScreen(screenId) {
  // Hide all screens
  document.querySelectorAll(".screen").forEach((screen) => {
    screen.classList.remove("active");
  });

  // Show target screen
  const screen = document.getElementById(screenId);
  if (screen) {
    screen.classList.add("active");
  } else {
    console.warn("Screen not found:", screenId);
  }
}

/**
 * Show/hide panels (for multi-panel layouts)
 * @param {string} panelId - ID of the panel to show
 * @param {boolean} updateUrl - Whether to update the URL search params (default: true)
 */
function showPanel(panelId, updateUrl = true) {
  // Hide all panels
  document.querySelectorAll(".panel").forEach((panel) => {
    panel.classList.remove("active");
  });

  // Show target panel
  const panel = document.getElementById(panelId);
  if (panel) {
    panel.classList.add("active");
  } else {
    console.warn("Panel not found:", panelId);
  }

  // Update sidebar navigation if it exists
  document.querySelectorAll(".sidebar-item").forEach((item) => {
    item.classList.remove("active");
  });

  // Find and activate the corresponding sidebar item
  document.querySelectorAll(".sidebar-item").forEach((item) => {
    const onclick = item.getAttribute("onclick");
    if (onclick?.includes(`showPanel('${panelId}')`)) {
      item.classList.add("active");
    }
  });

  // Update URL search params for persistence across reloads
  if (updateUrl && panel) {
    const url = new URL(window.location);
    url.searchParams.set("panel", panelId);
    window.history.replaceState({}, "", url);
  }
}

/**
 * Restore panel from URL search params (call on page load)
 * @returns {boolean} true if a panel was restored from URL
 */
function restorePanelFromUrl() {
  const url = new URL(window.location);
  const panelId = url.searchParams.get("panel");
  if (panelId) {
    const panel = document.getElementById(panelId);
    if (panel) {
      showPanel(panelId, false); // Don't update URL again
      return true;
    }
  }
  return false;
}

/**
 * Error message display
 */
function showError(elementId, message) {
  const errorEl = document.getElementById(elementId);
  if (errorEl) {
    errorEl.textContent = message;
    errorEl.style.display = "block";
  }
}

function hideError(elementId) {
  const errorEl = document.getElementById(elementId);
  if (errorEl) {
    errorEl.style.display = "none";
  }
}

/**
 * Escape HTML to prevent XSS
 */
function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Generate a random ID
 */
function generateId(prefix = "") {
  const timestamp = Date.now().toString(36);
  const random = Math.random().toString(36).substring(2);
  return prefix ? `${prefix}_${timestamp}_${random}` : `${timestamp}_${random}`;
}

/**
 * Copy text to clipboard
 */
function copyToClipboard(text) {
  if (navigator.clipboard) {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        console.log("Copied to clipboard:", text);
        return true;
      })
      .catch((err) => {
        console.error("Failed to copy:", err);
        return false;
      });
  } else {
    // Fallback for older browsers
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    document.body.appendChild(textarea);
    textarea.select();
    try {
      document.execCommand("copy");
      console.log("Copied to clipboard:", text);
      return true;
    } catch (err) {
      console.error("Failed to copy:", err);
      return false;
    } finally {
      document.body.removeChild(textarea);
    }
  }
}

/**
 * Format timestamp
 */
function formatTime(date = new Date()) {
  return date.toLocaleTimeString();
}

/**
 * Debounce a function
 * @param {Function} fn - Function to debounce
 * @param {number} delay - Delay in milliseconds
 */
function debounce(fn, delay) {
  let timeoutId;
  return function (...args) {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn.apply(this, args), delay);
  };
}

/**
 * Timer countdown manager
 */
class CountdownTimer {
  constructor(elementId, onComplete) {
    this.elementId = elementId;
    this.onComplete = onComplete;
    this.interval = null;
    this.deadline = null;
    this.serverTimeOffset = 0; // Difference between server time and local time
  }

  /**
   * Start countdown to a deadline
   * @param {string} deadlineISO - ISO8601 timestamp
   * @param {string} serverNowISO - Server's current time (for clock sync)
   */
  start(deadlineISO, serverNowISO) {
    this.stop(); // Clear any existing timer

    if (!deadlineISO) {
      this.hide();
      return;
    }

    // Calculate server time offset
    const serverNow = new Date(serverNowISO);
    const localNow = new Date();
    this.serverTimeOffset = serverNow.getTime() - localNow.getTime();

    this.deadline = new Date(deadlineISO);
    this.update();
    this.interval = setInterval(() => this.update(), 100); // Update 10x per second
  }

  stop() {
    if (this.interval) {
      clearInterval(this.interval);
      this.interval = null;
    }
  }

  hide() {
    const el = document.getElementById(this.elementId);
    if (el) {
      el.textContent = "";
      el.style.display = "none";
    }
  }

  update() {
    const el = document.getElementById(this.elementId);
    if (!el || !this.deadline) return;

    // Use server-synchronized time
    const now = new Date(Date.now() + this.serverTimeOffset);
    const remaining = this.deadline.getTime() - now.getTime();

    if (remaining <= 0) {
      el.textContent = "00:00";
      el.style.display = "block";
      this.stop();
      if (this.onComplete) {
        this.onComplete();
      }
      return;
    }

    const totalSeconds = Math.ceil(remaining / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;

    el.textContent = `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
    el.style.display = "block";
  }

  /**
   * Update deadline from server (e.g., when extended)
   * @param {string} deadlineISO - New deadline ISO8601 timestamp
   * @param {string} serverNowISO - Server's current time (for clock sync)
   */
  updateDeadline(deadlineISO, serverNowISO) {
    if (!deadlineISO) {
      this.hide();
      return;
    }

    // Update server time offset
    const serverNow = new Date(serverNowISO);
    const localNow = new Date();
    this.serverTimeOffset = serverNow.getTime() - localNow.getTime();

    // Update deadline
    this.deadline = new Date(deadlineISO);

    // If timer was stopped, restart it
    if (!this.interval) {
      this.update();
      this.interval = setInterval(() => this.update(), 100);
    }
  }
}

/**
 * QR Code generation utility
 */
const QRCodeManager = {
  /**
   * Generate a QR code for a URL
   * @param {string} containerId - ID of the element to render QR code in
   * @param {string} url - URL to encode in QR code
   * @param {object} options - QR code options (width, height, colorDark, colorLight)
   */
  generate(containerId, url, options = {}) {
    const container = document.getElementById(containerId);
    if (!container) {
      console.error("QR code container not found:", containerId);
      return;
    }

    // Clear existing QR code
    container.innerHTML = "";

    // Check if QRCode library is loaded
    if (typeof QRCode === "undefined") {
      console.error("QRCode library not loaded");
      container.innerHTML =
        '<p style="color: red;">QR code library not loaded</p>';
      return;
    }

    const defaultOptions = {
      width: 256,
      height: 256,
      colorDark: "#000000",
      colorLight: "#ffffff",
    };

    const qrOptions = { ...defaultOptions, ...options };

    try {
      new QRCode(container, {
        text: url,
        width: qrOptions.width,
        height: qrOptions.height,
        colorDark: qrOptions.colorDark,
        colorLight: qrOptions.colorLight,
        correctLevel: QRCode.CorrectLevel.H, // High error correction
      });
    } catch (error) {
      console.error("Failed to generate QR code:", error);
      container.innerHTML =
        '<p style="color: red;">Failed to generate QR code</p>';
    }
  },

  /**
   * Get the full URL for the audience join page
   */
  getAudienceJoinUrl() {
    const protocol = window.location.protocol;
    const host = window.location.host || window.location.hostname;
    return `${protocol}//${host}/`;
  },

  /**
   * Get the full URL for the player join page (with optional token)
   */
  getPlayerJoinUrl(token = null) {
    const protocol = window.location.protocol;
    const host = window.location.host || window.location.hostname;
    const baseUrl = `${protocol}//${host}/player.html`;
    return token ? `${baseUrl}?token=${token}` : baseUrl;
  },

  /**
   * Generate QR code for audience join
   */
  generateAudienceQR(containerId, options = {}) {
    const url = this.getAudienceJoinUrl();
    this.generate(containerId, url, options);
    return url;
  },

  /**
   * Generate QR code for player join
   */
  generatePlayerQR(containerId, token = null, options = {}) {
    const url = this.getPlayerJoinUrl(token);
    this.generate(containerId, url, options);
    return url;
  },
};

/**
 * Text-to-Speech manager using browser API
 */
class TTSManager {
  constructor() {
    this.synthesis = window.speechSynthesis;
    this.currentUtterance = null;
  }

  /**
   * Speak text using browser TTS
   * @param {string} text - Text to speak
   * @param {object} options - TTS options (rate, pitch, volume, voice)
   */
  speak(text, options = {}) {
    if (!this.synthesis) {
      console.warn("Speech synthesis not supported in this browser");
      return;
    }

    // Cancel any ongoing speech
    this.stop();

    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = options.lang || "de-DE";
    utterance.rate = options.rate || 1.0;
    utterance.pitch = options.pitch || 1.0;
    utterance.volume = options.volume || 1.0;

    // Select voice if specified
    if (options.voiceName) {
      const voices = this.synthesis.getVoices();
      const voice = voices.find((v) => v.name === options.voiceName);
      if (voice) {
        utterance.voice = voice;
      }
    }

    this.currentUtterance = utterance;
    this.synthesis.speak(utterance);
  }

  /**
   * Stop any ongoing speech
   */
  stop() {
    if (this.synthesis) {
      this.synthesis.cancel();
    }
    this.currentUtterance = null;
  }

  /**
   * Check if currently speaking
   */
  isSpeaking() {
    return this.synthesis?.speaking;
  }
}

if (typeof window !== "undefined") {
  Object.assign(window, {
    WSConnection,
    updateConnectionStatus,
    showScreen,
    showPanel,
    restorePanelFromUrl,
    showError,
    hideError,
    escapeHtml,
    generateId,
    copyToClipboard,
    formatTime,
    debounce,
    CountdownTimer,
    QRCodeManager,
    TTSManager,
  });
}
