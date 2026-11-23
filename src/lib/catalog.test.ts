// Tests for the step summaries shown in the step list.
import { describe, expect, it } from "vitest";
import { RULE_BY_KIND, RULES, stepSummary } from "./catalog";

describe("stepSummary", () => {
  it("shows the new name for Name", () => {
    expect(stepSummary({ kind: "name", mode: "fixed", value: "Lisbon" })).toBe(
      "Lisbon",
    );
  });

  it("falls back to the chosen action when no text is set", () => {
    expect(stepSummary({ kind: "name", mode: "reverse", value: "" })).toBe(
      "Reverse the name",
    );
    expect(stepSummary({ kind: "extension", mode: "lower", value: "" })).toBe(
      "lower case",
    );
  });

  it("ignores text in fields that are hidden for the chosen action", () => {
    expect(
      stepSummary({ kind: "extension", mode: "lower", value: "png" }),
    ).toBe("lower case");
  });

  it("shows the first number for decimal Numbers", () => {
    const r = { ...RULE_BY_KIND.numbering.defaults(), start: 1, pad: 3 };
    expect(stepSummary(r)).toBe("001");
  });

  it("shows the first replacement and how many more", () => {
    expect(
      stepSummary({
        kind: "replace",
        pairs: [
          { find: "IMG_", with: "" },
          { find: "_", with: " " },
          { find: "", with: "" },
        ],
      }),
    ).toBe("IMG_ →  +1");
    expect(
      stepSummary({ kind: "replace", pairs: [{ find: "", with: "" }] }),
    ).toBe("");
  });

  it("returns a string for every step's defaults", () => {
    for (const r of RULES)
      expect(typeof stepSummary(r.defaults())).toBe("string");
  });
});
