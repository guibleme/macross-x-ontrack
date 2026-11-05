import path from "node:path";
import { fileURLToPath } from "node:url";
import { app, BrowserWindow, dialog, shell } from "electron";
import { ensureSidecar } from "./sidecar.mjs";

const directory = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(directory, "../..");
const port = Number(process.env.MACROSS_PORT || 4280);
const stateRoot = path.resolve(
  process.env.MACROSS_STATE_ROOT || path.join(repository, "state"),
);
const smoke = process.env.MACROSS_ELECTRON_SMOKE === "1";
let origin;
function external(url) {
  try {
    if (new URL(url).protocol === "https:") void shell.openExternal(url);
  } catch {
    /* Invalid URLs do not leave the application. */
  }
}
async function start() {
  if (!Number.isInteger(port) || port < 1024 || port > 65535)
    throw new Error("Choose a local port from 1024 to 65535.");
  origin = `http://127.0.0.1:${port}`;
  const server = await ensureSidecar({
    stateRoot,
    port,
    script: path.resolve(directory, "../server/index.mjs"),
  });
  const window = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 700,
    minHeight: 600,
    show: false,
    backgroundColor: "#f7f6f2",
    title: "Macross × Ontrack",
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
      webSecurity: true,
    },
  });
  window.webContents.setWindowOpenHandler(({ url }) => {
    external(url);
    return { action: "deny" };
  });
  window.webContents.on("will-navigate", (event, url) => {
    try {
      if (new URL(url).origin === origin) return;
    } catch {
      /* Reject malformed navigation. */
    }
    event.preventDefault();
    external(url);
  });
  window.webContents.on("will-attach-webview", (event) =>
    event.preventDefault(),
  );
  if (!smoke) window.once("ready-to-show", () => window.show());
  await window.loadURL(origin);
  if (smoke) {
    const title = await window.webContents.executeJavaScript("document.title");
    process.stdout.write(
      `${JSON.stringify({ server, title, port, stateRoot })}\n`,
    );
    window.destroy();
    app.quit();
  }
}
app.on("window-all-closed", () => app.quit());
app
  .whenReady()
  .then(start)
  .catch((error) => {
    const message =
      error instanceof Error
        ? error.message
        : "The local application could not start.";
    if (smoke) process.stderr.write(`${message}\n`);
    else dialog.showErrorBox("Macross × Ontrack", message);
    app.exit(1);
  });
