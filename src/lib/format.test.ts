import { describe, expect, it } from "vitest";
import { alignmentBlocks, chartRows, cover, evalue, isNcbiOffPeak, score } from "./format";
import type { Hsp } from "../types";

const hsp = (over: Partial<Hsp>): Hsp => ({
  bitScore: 100,
  score: 50,
  evalue: 0,
  identity: 10,
  alignLen: 10,
  gaps: 0,
  queryFrom: 1,
  queryTo: 10,
  hitFrom: 3,
  hitTo: 12,
  queryStrand: "Plus",
  hitStrand: "Plus",
  qseq: "",
  hseq: "",
  midline: "",
  ...over,
});

describe("NCBI number formats", () => {
  it("rounds bit scores down like NCBI", () => {
    expect(score(1175.98)).toBe("1175");
    expect(score(1176)).toBe("1176");
  });
  it("formats E values", () => {
    expect(evalue(0)).toBe("0.0");
    expect(evalue(3.2e-45)).toBe("3e-45");
    expect(evalue(0.002)).toBe("0.002");
    expect(evalue(0.5)).toBe("0.50");
  });
  it("shows query cover as whole percent", () => {
    expect(cover(99.68)).toBe("100%");
  });
});

describe("alignmentBlocks", () => {
  it("numbers Query and Sbjct like the screenshot (Query 1-60, Sbjct 3-62)", () => {
    const seq = "A".repeat(120);
    const blocks = alignmentBlocks(hsp({ qseq: seq, hseq: seq, midline: "|".repeat(120), queryTo: 120, hitFrom: 3, hitTo: 122 }));
    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toMatchObject({ qStart: 1, qEnd: 60, sStart: 3, sEnd: 62 });
    expect(blocks[1]).toMatchObject({ qStart: 61, qEnd: 120, sStart: 63, sEnd: 122 });
  });
  it("does not count gaps and counts down on the Minus strand", () => {
    const blocks = alignmentBlocks(
      hsp({ qseq: "AC-GT", hseq: "ACAGT", midline: "|| ||", queryTo: 4, hitFrom: 50, hitTo: 46, hitStrand: "Minus" }),
    );
    expect(blocks[0]).toMatchObject({ qStart: 1, qEnd: 4, sStart: 50, sEnd: 46 });
  });
});

describe("chartRows", () => {
  it("folds species beyond the limit into Other", () => {
    const chart = Array.from({ length: 18 }, (_, i) => ({ species: `S${i}`, count: 18 - i, pct: 1 }));
    const { rows, other } = chartRows(chart, 15);
    expect(rows).toHaveLength(16);
    expect(other).toHaveLength(3);
    expect(rows[15].count).toBe(3 + 2 + 1);
    expect(chartRows(chart, 15, true).rows).toHaveLength(18);
  });
});

describe("isNcbiOffPeak", () => {
  it("knows US Eastern night and weekends", () => {
    expect(isNcbiOffPeak(new Date("2026-11-04T03:00:00Z"))).toBe(true); // Tue 22:00 ET
    expect(isNcbiOffPeak(new Date("2026-11-04T17:00:00Z"))).toBe(false); // Wed 12:00 ET
    expect(isNcbiOffPeak(new Date("2026-11-07T17:00:00Z"))).toBe(true); // Saturday
  });
});
