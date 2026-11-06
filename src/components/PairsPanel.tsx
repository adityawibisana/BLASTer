import { useState } from "react";
import { ArrowLeftRight, Link2, Unlink, X } from "lucide-react";
import { api } from "../api";
import { useApp } from "../state";
import { Badge } from "./ui";

/** Forward/reverse pairs: suggestions to confirm, confirmed pairs, manual pairing. */
export function PairsPanel() {
  const { project, run } = useApp();
  const [fwd, setFwd] = useState("");
  const [rev, setRev] = useState("");
  if (!project) return null;

  const free = project.reads.filter((r) => !r.pair && !r.pairedWith);
  const pairs = project.reads.filter((r) => r.pair);
  const sugg = project.pairSuggestions;

  return (
    <div className="card px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="font-semibold">Forward / reverse pairs</div>
        <div className="label">
          {pairs.length} pair{pairs.length === 1 ? "" : "s"} · {free.length} single reads
        </div>
      </div>

      {sugg.length === 0 && pairs.length === 0 && (
        <div className="label mt-1">
          No pairs found: every file is treated as a single read. Pairs are suggested when names share a stem with a
          direction marker (e.g. <span className="font-mono">S01_F</span> / <span className="font-mono">S01_R</span>), the
          detected directions are opposite, and the reads overlap.
        </div>
      )}

      {sugg.length > 0 && (
        <div className="mt-2 flex flex-col gap-1.5">
          <div className="label">Suggested pairs: confirm or reject each one</div>
          {sugg.map((s) => (
            <div key={`${s.forwardId}|${s.reverseId}`} className="flex items-center gap-3 rounded-md bg-info-soft px-3 py-1.5">
              <span className="font-medium">{s.forwardId}</span>
              <Badge>F</Badge>
              <span>+</span>
              <span className="font-medium">{s.reverseId}</span>
              <Badge>R</Badge>
              <span className="text-muted">
                overlap {s.overlap} bp, {s.identity.toFixed(1)}% identical
              </span>
              <div className="ml-auto flex gap-1.5">
                <button className="btn btn-sm btn-primary" onClick={() => run(() => api.confirmPair(s.forwardId, s.reverseId), "Pair confirmed")}>
                  <Link2 size={13} /> Pair
                </button>
                <button className="btn btn-sm" onClick={() => run(() => api.rejectPair(s.forwardId, s.reverseId))}>
                  <X size={13} /> Reject
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {pairs.length > 0 && (
        <div className="mt-2 flex flex-col gap-1.5">
          <div className="label">Confirmed pairs (each is one sample, searched as its consensus)</div>
          {pairs.map((r) => {
            const p = r.pair!;
            return (
              <div key={r.id} className="flex flex-wrap items-center gap-3 rounded-md border border-line px-3 py-1.5">
                <span className="font-medium">{r.id}</span>
                <Badge>F</Badge>
                <span>+</span>
                <span className="font-medium">{p.reverseId}</span>
                <Badge>R</Badge>
                {p.noOverlap ? (
                  <>
                    <Badge tone="bad">do not overlap</Badge>
                    <span className="text-muted">Choose which read to search:</span>
                    <button className={`btn btn-sm ${p.useOnly === "F" ? "btn-primary" : ""}`} onClick={() => run(() => api.setPairUseOnly(r.id, "F"))}>
                      Forward only
                    </button>
                    <button className={`btn btn-sm ${p.useOnly === "R" ? "btn-primary" : ""}`} onClick={() => run(() => api.setPairUseOnly(r.id, "R"))}>
                      Reverse only
                    </button>
                  </>
                ) : (
                  <span className="text-muted">
                    consensus {r.derived.finalLen} bp · overlap {p.overlap} bp · {p.identity.toFixed(1)}% identical · {p.disagreements} disagreements
                    {p.conflicts.length > 0 && <span className="text-warn"> · {p.conflicts.length} conflict{p.conflicts.length > 1 ? "s" : ""}</span>}
                  </span>
                )}
                <div className="ml-auto flex gap-1.5">
                  <button className="btn btn-sm" title="The reverse read becomes the forward one and vice versa" onClick={() => run(() => api.swapPair(r.id))}>
                    <ArrowLeftRight size={13} /> Swap F/R
                  </button>
                  <button className="btn btn-sm" onClick={() => run(() => api.unpair(r.id), "Pair split into two single reads")}>
                    <Unlink size={13} /> Unpair
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-line pt-3">
        <span className="label">Pair two files by hand:</span>
        <select className="input" value={fwd} onChange={(e) => setFwd(e.target.value)}>
          <option value="">Forward read…</option>
          {free.map((r) => (
            <option key={r.id} value={r.id}>
              {r.id} ({r.derived.direction ?? "?"})
            </option>
          ))}
        </select>
        <span>+</span>
        <select className="input" value={rev} onChange={(e) => setRev(e.target.value)}>
          <option value="">Reverse read…</option>
          {free
            .filter((r) => r.id !== fwd)
            .map((r) => (
              <option key={r.id} value={r.id}>
                {r.id} ({r.derived.direction ?? "?"})
              </option>
            ))}
        </select>
        <button
          className="btn btn-sm"
          disabled={!fwd || !rev}
          onClick={async () => {
            await run(() => api.confirmPair(fwd, rev), "Pair confirmed");
            setFwd("");
            setRev("");
          }}
        >
          <Link2 size={13} /> Pair
        </button>
      </div>
    </div>
  );
}
