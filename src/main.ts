// Front-end entry point: mounts the Alterix app into the page.
import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";
import { invoke } from "@tauri-apps/api/core";

// Send errors that nothing else caught to Alterix.log.
const logError = (message: string) =>
  invoke("log_line", { level: "error", area: "ui", message }).catch(() => {});
window.addEventListener("error", (e) =>
  logError(
    `${e.message} at ${e.filename}:${e.lineno}:${e.colno}\n${e.error?.stack ?? ""}`,
  ),
);
window.addEventListener("unhandledrejection", (e) =>
  logError(
    `Unhandled promise rejection: ${e.reason instanceof Error ? `${e.reason.message}\n${e.reason.stack}` : String(e.reason)}`,
  ),
);

export default mount(App, { target: document.getElementById("app")! });
