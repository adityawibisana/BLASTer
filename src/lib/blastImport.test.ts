import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { importMessage } = await import("./blastImport");

const base = { samples: 0, groups: [], unknown: [], changed: [], already: [], stillMissing: 0 };

describe("importMessage", () => {
  it("says how many samples got results and what is left", () => {
    expect(importMessage({ ...base, samples: 17, groups: ["g0001"], stillMissing: 1 })).toEqual({
      ok: true,
      text: "Results for 17 samples imported. 1 sample still needs a search.",
    });
    expect(importMessage({ ...base, samples: 1 }).text).toBe("Results for 1 sample imported. Every sample now has results.");
  });

  it("explains a file for other samples", () => {
    const m = importMessage({ ...base, unknown: ["Achl_1", "Achl_2", "Achl_3", "Achl_4"] });
    expect(m.ok).toBe(false);
    expect(m.text).toBe("Nothing imported: the file is for other samples. 4 queries in the file match no sample of this project (Achl_1, Achl_2, Achl_3 and 1 more).");
  });

  it("lists samples that already had results or changed", () => {
    const m = importMessage({ ...base, samples: 2, already: ["a"], changed: ["b", "c"] });
    expect(m.text).toContain("1 sample already had results");
    expect(m.text).toContain("2 samples changed since the file was searched (b, c)");
  });
});
