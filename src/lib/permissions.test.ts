// Checks that the window permissions cover what the UI calls, so the window can always close.
import { describe, expect, it } from "vitest";
import capabilityText from "../../src-tauri/capabilities/default.json?raw";
import app from "../App.svelte?raw";

const capability = JSON.parse(capabilityText) as {
  permissions: (string | { identifier: string })[];
};
const permissions = capability.permissions.map((p) =>
  typeof p === "string" ? p : p.identifier,
);

describe("window permissions", () => {
  // onCloseRequested stops the normal close and calls destroy() itself after the handler.
  // Without this permission destroy() is refused and the window never closes.
  it("allow destroy when the app listens for the close request", () => {
    if (app.includes("onCloseRequested"))
      expect(permissions).toContain("core:window:allow-destroy");
  });
});
