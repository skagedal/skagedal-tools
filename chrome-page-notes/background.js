const HOST_NAME = "com.skagedal.chrome_page_notes_host";

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (!changeInfo.url) {
    return;
  }
  chrome.runtime.sendNativeMessage(
    HOST_NAME,
    { action: "url_visited", url: changeInfo.url },
    () => {
      if (chrome.runtime.lastError) {
        console.error("Native messaging failed:", chrome.runtime.lastError.message);
      }
    },
  );
});
