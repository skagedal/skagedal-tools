function clear(container) {
  container.textContent = "";
}

function render(url, response) {
  const container = document.getElementById("note");
  clear(container);

  if (response.status === "error") {
    container.textContent = `Error: ${response.message}`;
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
          updateBadge(tabId, createResponse.note);
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
      updateBadge(tabId, response.note);
    })
    .catch((error) => {
      document.getElementById("note").textContent = `Error: ${error.message}`;
    });
});
