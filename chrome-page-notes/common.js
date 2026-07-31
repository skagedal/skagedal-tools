const HOST_NAME = "tech.skagedal.chrome_page_notes_host";
const BADGE_COLOR = "#8b5cf6";
const ERROR_BADGE_COLOR = "#dc2626";

function sendToHost(message) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendNativeMessage(HOST_NAME, message, (response) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
        return;
      }
      resolve(response);
    });
  });
}

// `response` is the host's {status, note} payload, or null/undefined when
// native messaging itself failed (e.g. the host process couldn't be
// reached at all, so there's no response to inspect).
function updateBadge(tabId, response) {
  if (!response || response.status === "error") {
    // Badge text renders in a tiny monochrome font, and multi-color emoji
    // like "❌" don't render legibly there — a plain glyph does.
    chrome.action.setBadgeText({ tabId, text: "!" });
    chrome.action.setBadgeBackgroundColor({ tabId, color: ERROR_BADGE_COLOR });
    return;
  }
  chrome.action.setBadgeText({ tabId, text: response.note?.exists ? "✓" : "" });
  chrome.action.setBadgeBackgroundColor({ tabId, color: BADGE_COLOR });
}
