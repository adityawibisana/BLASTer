import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ZoomIn, ZoomOut } from "lucide-react";
import type { PrimerScan, Range, TraceView } from "../types";

const TRACE_TOP = 22;
const TRACE_H = 160;
const BASE_Y = TRACE_TOP + TRACE_H + 16;
const QUAL_TOP = BASE_Y + 8;
const QUAL_H = 26;
const HEIGHT = QUAL_TOP + QUAL_H + 6;
const MIN_KEEP = 20;

function cssVar(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Peak positions; evenly spaced if the file has none. */
function usePeaks(trace: TraceView) {
  return useMemo(() => {
    if (trace.peaks.length === trace.bases.length) return trace.peaks;
    return Array.from({ length: trace.bases.length }, (_, i) => i * 12 + 6);
  }, [trace]);
}

export function Chromatogram({
  trace,
  range,
  primer,
  hqQuality,
  onDrag,
  onCommit,
}: {
  trace: TraceView;
  range: Range;
  primer: PrimerScan;
  hqQuality: number;
  onDrag: (r: Range) => void;
  onCommit: (r: Range) => void;
}) {
  const peaks = usePeaks(trace);
  const n = trace.bases.length;
  const traceLen = Math.max(trace.traces[0].length, (peaks[n - 1] ?? 0) + 12);
  const [zoom, setZoom] = useState(1.5);
  const [width, setWidth] = useState(800);
  const [scroll, setScroll] = useState(0);
  const [drag, setDrag] = useState<"start" | "end" | null>(null);
  const [hover, setHover] = useState(false);
  const canvas = useRef<HTMLCanvasElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const live = useRef(range);
  live.current = range;

  /** x (in trace points) of the boundary just before base i. */
  const boundary = useCallback(
    (i: number) => {
      if (n === 0) return 0;
      if (i <= 0) return peaks[0] - 6;
      if (i >= n) return peaks[n - 1] + 6;
      return (peaks[i - 1] + peaks[i]) / 2;
    },
    [peaks, n],
  );

  /** Nearest base boundary to a trace position. */
  const nearestBoundary = useCallback(
    (pt: number) => {
      let lo = 0;
      let hi = n;
      while (lo < hi) {
        const mid = (lo + hi) >> 1;
        if (boundary(mid) < pt) lo = mid + 1;
        else hi = mid;
      }
      if (lo > 0 && Math.abs(boundary(lo - 1) - pt) < Math.abs(boundary(lo) - pt)) return lo - 1;
      return lo;
    },
    [boundary, n],
  );

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // Show the start of the kept region when a new read is opened.
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollLeft = Math.max(0, boundary(range.start) * zoom - 80);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trace]);

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.floor(width * dpr);
    c.height = Math.floor(HEIGHT * dpr);
    c.style.width = `${width}px`;
    c.style.height = `${HEIGHT}px`;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, HEIGHT);

    const colors = {
      A: cssVar("--base-a"),
      C: cssVar("--base-c"),
      G: cssVar("--base-g"),
      T: cssVar("--base-t"),
    };
    const X = (pt: number) => pt * zoom - scroll;
    const p0 = Math.max(0, Math.floor(scroll / zoom) - 2);
    const p1 = Math.min(traceLen, Math.ceil((scroll + width) / zoom) + 2);

    // Trimmed-off ends and primer sites.
    g.fillStyle = cssVar("--shade");
    g.fillRect(0, 0, Math.max(0, X(boundary(range.start))), HEIGHT);
    g.fillRect(X(boundary(range.end)), 0, width, HEIGHT);
    g.fillStyle = cssVar("--primer");
    for (const site of [primer.startSite, primer.endSite]) {
      if (!site) continue;
      const a = X(boundary(site.start));
      const b = X(boundary(site.end));
      g.fillRect(a, 0, b - a, HEIGHT);
      g.fillStyle = cssVar("--warn");
      g.font = "600 10.5px Segoe UI, sans-serif";
      g.fillText(`${site.primer}${site.partial ? " (partial)" : ""}`, a + 3, TRACE_TOP + 10);
      g.fillStyle = cssVar("--primer");
    }

    // Traces, scaled to the visible window.
    let max = 1;
    for (const ch of trace.traces) for (let i = p0; i < p1 && i < ch.length; i++) if (ch[i] > max) max = ch[i];
    const Y = (v: number) => TRACE_TOP + TRACE_H - (Math.max(0, v) / max) * (TRACE_H - 6);
    (["A", "C", "G", "T"] as const).forEach((b, ci) => {
      const ch = trace.traces[ci];
      if (!ch.length) return;
      g.strokeStyle = colors[b];
      g.lineWidth = 1.2;
      g.beginPath();
      const step = zoom < 0.6 ? 2 : 1;
      for (let i = p0; i < p1 && i < ch.length; i += step) {
        const x = X(i);
        const y = Y(ch[i]);
        if (i === p0) g.moveTo(x, y);
        else g.lineTo(x, y);
      }
      g.stroke();
    });

    // Base calls, quality bars, position ruler.
    const spacing = n > 1 ? ((peaks[n - 1] - peaks[0]) / (n - 1)) * zoom : 12;
    const showLetters = spacing >= 7;
    g.font = "600 11.5px Cascadia Mono, Consolas, monospace";
    g.textAlign = "center";
    for (let i = 0; i < n; i++) {
      const x = X(peaks[i]);
      if (x < -10 || x > width + 10) continue;
      const q = trace.quals[i] ?? 0;
      const qc = q >= hqQuality ? cssVar("--q-high") : q >= 20 ? cssVar("--q-med") : cssVar("--q-low");
      const b = trace.bases[i] as keyof typeof colors;
      if (showLetters) {
        g.fillStyle = colors[b] ?? cssVar("--muted");
        g.fillText(trace.bases[i], x, BASE_Y);
      }
      g.fillStyle = qc;
      const h = Math.min(1, q / 60) * QUAL_H;
      g.fillRect(x - Math.max(1, spacing / 2 - 1), QUAL_TOP + QUAL_H - h, Math.max(1.5, spacing - 2), h);
      if ((i + 1) % 10 === 0 && spacing * 10 >= 40) {
        g.fillStyle = cssVar("--muted");
        g.font = "10.5px Segoe UI, sans-serif";
        g.fillText(String(i + 1), x, 12);
        g.font = "600 11.5px Cascadia Mono, Consolas, monospace";
      }
    }
    g.textAlign = "left";

    // Drag handles for the kept region.
    for (const [which, i] of [
      ["start", range.start],
      ["end", range.end],
    ] as const) {
      const x = X(boundary(i));
      g.strokeStyle = cssVar("--accent");
      g.lineWidth = drag === which ? 3 : 2;
      g.beginPath();
      g.moveTo(x, 0);
      g.lineTo(x, HEIGHT);
      g.stroke();
      g.fillStyle = cssVar("--accent");
      g.beginPath();
      g.roundRect(x - 6, TRACE_TOP + 20, 12, 30, 3);
      g.fill();
      g.strokeStyle = "#fff";
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(x - 2, TRACE_TOP + 28);
      g.lineTo(x - 2, TRACE_TOP + 42);
      g.moveTo(x + 2, TRACE_TOP + 28);
      g.lineTo(x + 2, TRACE_TOP + 42);
      g.stroke();
    }
  }, [trace, peaks, n, traceLen, zoom, width, scroll, range, primer, boundary, drag, hqQuality]);

  const ptAt = (clientX: number) => {
    const el = scroller.current!;
    const rect = el.getBoundingClientRect();
    return (clientX - rect.left + el.scrollLeft) / zoom;
  };
  const handleAt = (clientX: number): "start" | "end" | null => {
    const pt = ptAt(clientX);
    const tol = 9 / zoom;
    if (Math.abs(pt - boundary(live.current.start)) < tol) return "start";
    if (Math.abs(pt - boundary(live.current.end)) < tol) return "end";
    return null;
  };

  const scrollTo = (i: number) => {
    const el = scroller.current;
    if (el) el.scrollTo({ left: Math.max(0, boundary(i) * zoom - el.clientWidth / 2), behavior: "smooth" });
  };

  const changeZoom = (factor: number) => {
    const el = scroller.current;
    const center = el ? (el.scrollLeft + el.clientWidth / 2) / zoom : 0;
    const z = Math.min(4, Math.max(0.3, zoom * factor));
    setZoom(z);
    requestAnimationFrame(() => {
      if (el) el.scrollLeft = center * z - el.clientWidth / 2;
    });
  };

  return (
    <div>
      <div className="mb-2 flex items-center gap-2">
        <button className="btn btn-sm" onClick={() => scrollTo(range.start)}>
          Go to start limit
        </button>
        <button className="btn btn-sm" onClick={() => scrollTo(range.end)}>
          Go to end limit
        </button>
        <div className="ml-auto flex items-center gap-1">
          <button className="btn btn-sm" title="Zoom out" onClick={() => changeZoom(1 / 1.4)}>
            <ZoomOut size={14} />
          </button>
          <button className="btn btn-sm" title="Zoom in" onClick={() => changeZoom(1.4)}>
            <ZoomIn size={14} />
          </button>
        </div>
      </div>
      <div className="card relative overflow-hidden" style={{ height: HEIGHT + 14 }}>
        <canvas ref={canvas} className="pointer-events-none absolute top-0 left-0" />
        <div
          ref={scroller}
          className="absolute inset-0 overflow-x-auto overflow-y-hidden"
          style={{ cursor: drag || hover ? "col-resize" : "default" }}
          onScroll={(e) => setScroll(e.currentTarget.scrollLeft)}
          onPointerDown={(e) => {
            const h = handleAt(e.clientX);
            if (!h) return;
            e.currentTarget.setPointerCapture(e.pointerId);
            setDrag(h);
          }}
          onPointerMove={(e) => {
            if (!drag) {
              setHover(handleAt(e.clientX) !== null);
              return;
            }
            const i = nearestBoundary(ptAt(e.clientX));
            const r = live.current;
            const next =
              drag === "start"
                ? { start: Math.min(i, r.end - MIN_KEEP), end: r.end }
                : { start: r.start, end: Math.max(i, r.start + MIN_KEEP) };
            if (next.start !== r.start || next.end !== r.end) onDrag({ start: Math.max(0, next.start), end: Math.min(n, next.end) });
          }}
          onPointerUp={() => {
            if (drag) onCommit(live.current);
            setDrag(null);
          }}
        >
          <div style={{ width: traceLen * zoom, height: 1 }} />
        </div>
      </div>
      <div className="label mt-1.5 flex flex-wrap gap-x-4">
        <span>Drag the teal limits to change the kept region.</span>
        <span>
          <span className="inline-block h-2 w-3 align-middle" style={{ background: "var(--primer)" }} /> primer site
        </span>
        <span>
          <span className="inline-block h-2 w-3 align-middle" style={{ background: "var(--shade)" }} /> trimmed off
        </span>
        <span>
          Quality bars: <span style={{ color: "var(--q-high)" }}>high ≥ {hqQuality}</span> ·{" "}
          <span style={{ color: "var(--q-med)" }}>medium 20–{hqQuality - 1}</span> ·{" "}
          <span style={{ color: "var(--q-low)" }}>low &lt; 20</span>
        </span>
      </div>
    </div>
  );
}
