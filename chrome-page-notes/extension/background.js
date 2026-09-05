importScripts("common.js");

function checkAndBadge(tabId, url) {
  if (!url) {
    return;
  }
  sendToHost({ action: "url_visited", url })
    .then((response) => updateBadge(tabId, response))
    .catch((error) => {
      console.error("Native messaging failed:", error);
      updateBadge(tabId, null);
    });
}

// changeInfo.url is only set when the URL actually changes, so a reload of
// the same URL wouldn't trigger this if we keyed off that instead — use
// status "complete" (and tab.url, always present) to catch reloads too.
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === "complete") {
    checkAndBadge(tabId, tab.url);
  }
});

// Tabs that were already open before this feature existed (or before the
// extension was installed) never fired onUpdated, so re-check on focus too.
chrome.tabs.onActivated.addListener(({ tabId }) => {
  chrome.tabs.get(tabId, (tab) => checkAndBadge(tabId, tab?.url));
});
