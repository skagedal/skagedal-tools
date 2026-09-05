function clear(container) {
  container.textContent = "";
}

function render(url, response) {
  const container = document.getElementById("note");
  clear(container);

  if (response.status === "error") {
    const message = document.createElement("p");
    message.textContent = `Error: ${response.message}`;
    container.appendChild(message);

    const openAppButton = document.createElement("button");
    openAppButton.textContent = "Open Obsidian";
    openAppButton.addEventListener("click", () => {
      openAppButton.disabled = true;
      openAppButton.textContent = "Opening Obsidian…";
      sendToHost({ action: "open_app" }).catch((error) => console.error("Failed to open Obsidian:", error));
    });
    container.appendChild(openAppButton);
    return;
  }

  const note = response.note;
  if (!note) {
    container.textContent = "Notes are only supported for https:// pages.";
    return;
  }

  if (note.exists) {
    const pre = document.createElement("pre");
    pre.textContent = note.content;
    container.appendChild(pre);

    const openButton = document.createElement("button");
    openButton.textContent = "Open in Obsidian";
    openButton.addEventListener("click", () => {
      sendToHost({ action: "open_note", path: note.path }).catch((error) =>
        console.error("Failed to open note:", error),
      );
    });
    container.appendChild(openButton);
  } else {
    const message = document.createElement("p");
    message.textContent = "No note yet for this page.";
    container.appendChild(message);

    const createButton = document.createElement("button");
    createButton.textContent = "Create note";
    createButton.addEventListener("click", () => {
      sendToHost({ action: "create_note", url })
        .then((createResponse) => {
          render(url, createResponse);
          updateBadge(tabId, createResponse);
        })
        .catch((error) => console.error("Failed to create note:", error));
    });
    container.appendChild(createButton);
  }
}

let tabId;

chrome.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
  tabId = tab?.id;
  const url = tab?.url;
  sendToHost({ action: "activated", url })
    .then((response) => {
      render(url, response);
      updateBadge(tabId, response);
    })
    .catch((error) => {
      render(url, { status: "error", message: error.message });
      updateBadge(tabId, null);
    });
});
