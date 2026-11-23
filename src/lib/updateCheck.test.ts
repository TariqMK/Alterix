// Tests for the update check, with the network call stubbed.
import { describe, expect, it } from "vitest";
import {
  check,
  isNewer,
  kindOf,
  parseRelease,
  parseVersion,
  RELEASES_PAGE,
  summary,
  UpdateError,
} from "./updateCheck";

const fakeFetch = (status: number, body: unknown, bad = false) =>
  (async () =>
    ({
      status,
      ok: status >= 200 && status < 300,
      json: async () => {
        if (bad) throw new SyntaxError("bad json");
        return body;
      },
    }) as Response) as typeof fetch;

describe("versions", () => {
  it("parses plain versions only", () => {
    expect(parseVersion("v1.2.3")).toEqual([1, 2, 3]);
    expect(parseVersion(" 0.10.0 ")).toEqual([0, 10, 0]);
    expect(parseVersion("v0.0.0-dev")).toBeNull();
    expect(parseVersion("v1.5.0-new-sync.1")).toBeNull();
  });
  it("compares as numbers", () => {
    expect(isNewer("v0.10.0", "v0.9.0")).toBe(true);
    expect(isNewer("v1.0.0", "v1.0.0")).toBe(false);
    expect(isNewer("v1.0.0", "v0.0.0-dev")).toBe(false);
  });
  it("classifies dev and branch builds", () => {
    expect(kindOf("v0.0.0-dev", "v1.0.0")).toBe("dev");
    expect(kindOf("v1.5.0-new-sync.1", "v1.4.0")).toBe("branch");
    expect(kindOf("v1.4.0", "v1.5.0")).toBe("newer");
    expect(kindOf("v1.5.0", "v1.5.0")).toBe("current");
  });
});

describe("responses", () => {
  it("reads a sample release", () => {
    const info = parseRelease(
      {
        tag_name: "v1.2.0",
        html_url: "https://github.com/x/y/releases/tag/v1.2.0",
        body: " Notes ",
      },
      "v1.1.0",
    );
    expect(info).toEqual({
      current: "v1.1.0",
      latest: "v1.2.0",
      url: "https://github.com/x/y/releases/tag/v1.2.0",
      notes: "Notes",
      kind: "newer",
    });
    expect(summary(info).headline).toBe("Alterix v1.2.0 is available.");
  });
  it("falls back to the releases page for missing or non-https URLs", () => {
    expect(parseRelease({ tag_name: "1.0.0" }, "v1.0.0").url).toBe(
      RELEASES_PAGE,
    );
    expect(
      parseRelease({ tag_name: "1.0.0", html_url: "http://x" }, "v1.0.0").url,
    ).toBe(RELEASES_PAGE);
    expect(
      summary(parseRelease({ tag_name: "1.0.0" }, "v1.0.0")).headline,
    ).toBe("Alterix is up to date.");
  });
  it("rejects odd answers", () => {
    expect(() => parseRelease(null, "v1.0.0")).toThrow(UpdateError);
    expect(() => parseRelease({ tag_name: "latest" }, "v1.0.0")).toThrow(
      "can't read",
    );
  });
});

describe("errors", () => {
  it("maps each failure to its message", async () => {
    await expect(check("v1.0.0", fakeFetch(404, {}))).rejects.toThrow(
      "No Alterix releases are published yet.",
    );
    await expect(check("v1.0.0", fakeFetch(403, {}))).rejects.toThrow(
      "Try again in an hour.",
    );
    await expect(check("v1.0.0", fakeFetch(429, {}))).rejects.toThrow(
      "Try again in an hour.",
    );
    await expect(check("v1.0.0", fakeFetch(500, {}))).rejects.toThrow(
      "GitHub answered with error 500. Try again later.",
    );
    await expect(check("v1.0.0", fakeFetch(200, {}, true))).rejects.toThrow(
      "can't read",
    );
    const offline = (async () => {
      throw new TypeError("Load failed");
    }) as typeof fetch;
    await expect(check("v1.0.0", offline)).rejects.toThrow(
      "Can't reach GitHub: Load failed. Check the internet connection.",
    );
  });
  it("returns the dev-build result for a development version", async () => {
    const info = await check(
      "v0.0.0-dev",
      fakeFetch(200, { tag_name: "v1.0.0" }),
    );
    expect(summary(info).headline).toBe("This is a development build.");
  });
});
