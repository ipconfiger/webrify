// Interaction-telemetry recorder for behavior analysis.
//
// Accumulates mouse-move samples (x, y, t), inter-click intervals, and inter-key
// intervals while the widget is mounted. At solve time the worker turns these
// into a human-likeness score via the WASM `behavior_score` binding (the
// canonical CV-based logic lives in `turnstile_core::behavior`).
//
// Samples are capped to bound memory; only the most recent mouse positions are
// kept (enough for a stable speed-CV). Raw events never leave the browser —
// only the derived score is sent.

/** Maximum mouse samples retained (x,y,t triples → 3 numbers each). */
const MAX_MOUSE_TRIPLES = 100;

export interface BehaviorSnapshot {
  /** Flat `[x0,y0,t0, x1,y1,t1, …]`. */
  mouse: Float64Array;
  /** Inter-click intervals, ms. */
  clickIntervals: Float64Array;
  /** Inter-key intervals, ms. */
  keyIntervals: Float64Array;
}

export class BehaviorRecorder {
  private mouse: number[] = [];
  private clickIntervals: number[] = [];
  private keyIntervals: number[] = [];
  private lastClickT: number | null = null;
  private lastKeyT: number | null = null;
  private detach: (() => void) | null = null;

  start(): void {
    const onMove = (e: MouseEvent) => this.pushMouse(e.clientX, e.clientY);
    const onClick = () => this.pushInterval(this.clickIntervals, () => this.lastClickT, (t) => (this.lastClickT = t));
    const onKey = () => this.pushInterval(this.keyIntervals, () => this.lastKeyT, (t) => (this.lastKeyT = t));
    window.addEventListener("mousemove", onMove, { passive: true });
    window.addEventListener("click", onClick);
    window.addEventListener("keydown", onKey);
    this.detach = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("click", onClick);
      window.removeEventListener("keydown", onKey);
    };
  }

  stop(): void {
    this.detach?.();
    this.detach = null;
  }

  snapshot(): BehaviorSnapshot {
    return {
      mouse: Float64Array.from(this.mouse),
      clickIntervals: Float64Array.from(this.clickIntervals),
      keyIntervals: Float64Array.from(this.keyIntervals),
    };
  }

  private pushMouse(x: number, y: number): void {
    this.mouse.push(x, y, performance.now());
    // Trim oldest triples once over the cap.
    while (this.mouse.length > MAX_MOUSE_TRIPLES * 3) {
      this.mouse.splice(0, 3);
    }
  }

  private pushInterval(
    buf: number[],
    getter: () => number | null,
    setter: (t: number) => void,
  ): void {
    const t = performance.now();
    const prev = getter();
    if (prev !== null) buf.push(t - prev);
    setter(t);
  }
}
