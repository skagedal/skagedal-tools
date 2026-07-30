const HOST_NAME = "tech.skagedal.chrome_page_notes_host";
const BADGE_COLOR = "#8b5cf6";

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

function updateBadge(tabId, note) {
  chrome.action.setBadgeText({ tabId, text: note?.exists ? "✓" : "" });
  chrome.action.setBadgeBackgroundColor({ tabId, color: BADGE_COLOR });
}
