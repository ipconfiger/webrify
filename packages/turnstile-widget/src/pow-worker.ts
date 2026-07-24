// Web Worker: loads the WASM module and runs fingerprint hashing, behavior
// scoring, and PoW off the main thread so the tight SHA-256 loops never freeze
// the UI. The wasm pkg is copied into `../wasm/` by the `build-widget` justfile
// recipe before `vite build`.

import init, { behavior_score, fingerprint_hash, solve_challenge } from "../wasm/turnstile_wasm.js";

interface SolveRequest {
  challenge: string;
  difficulty: number;
  maxnumber: number;
  /** Optional nonce range for multi-worker mode. */
  nonceStart?: number;
  nonceEnd?: number;
  signalsJson: string;
  fingerprintEnabled: boolean;
  /** Flat `[x0,y0,t0, x1,y1,t1, …]`. Transferred (detached) by the caller. */
  mouse: Float64Array;
  clickIntervals: Float64Array;
  keyIntervals: Float64Array;
  /** Flat `[x0,y0, x1,y1, …]` click position pairs. */
  clickPositions: Float64Array;
}

self.onmessage = async (e: MessageEvent<SolveRequest>) => {
  const {
    challenge,
    difficulty,
    maxnumber,
    signalsJson,
    fingerprintEnabled,
    mouse,
    clickIntervals,
    keyIntervals,
    clickPositions,
  } = e.data;
  try {
    await init();
    const fingerprint = fingerprintEnabled ? fingerprint_hash(signalsJson) : null;

    // Estimate expected solve time from difficulty: ~2^(diff-3) iterations,
    // each taking ~2µs in WASM. Start a progress heartbeat based on elapsed
    // wall-clock time, sending an estimate back to the main thread.
    const expectedMs = Math.pow(2, difficulty) * 0.002;
    const t0 = performance.now();
    const progressTimer = setInterval(() => {
      const elapsed = performance.now() - t0;
      const pct = Math.min(99, Math.round((elapsed / expectedMs) * 100));
      (self as unknown as Worker).postMessage({ progress: pct });
    }, 200);

    // Multi-worker mode: use solve_challenge_range when start/end are given.
    // Falls back to solve_challenge (full range) for single-worker mode.
    let nonce: number;
    const hasRange = typeof e.data.nonceStart === "number" && typeof e.data.nonceEnd === "number";
    if (hasRange) {
      // TODO: import and use solve_challenge_range from rebuilt WASM
      // For now, search the full range but report which sub-range was searched
      nonce = solve_challenge(challenge, fingerprint, difficulty, maxnumber);
      // Clamp to range — only report if nonce falls within our assigned range
      if (nonce < e.data.nonceStart! || nonce > e.data.nonceEnd!) {
        (self as unknown as Worker).postMessage({ ok: false, exhausted: true });
        return;
      }
    } else {
      nonce = solve_challenge(challenge, fingerprint, difficulty, maxnumber);
    }
    clearInterval(progressTimer);

    // Behavior: human-likeness score (0-100) or null if too little signal.
    const behaviorScore = behavior_score(mouse, clickIntervals, keyIntervals, clickPositions);
    (self as unknown as Worker).postMessage({
      ok: true,
      nonce,
      fingerprint,
      behaviorScore,
    });
  } catch (err) {
    (self as unknown as Worker).postMessage({
      ok: false,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};
