// Web Worker: loads the WASM module and runs fingerprint hashing, behavior
// scoring, and PoW off the main thread so the tight SHA-256 loops never freeze
// the UI. The wasm pkg is copied into `../wasm/` by the `build-widget` justfile
// recipe before `vite build`.

import init, { behavior_score, fingerprint_hash, solve_challenge } from "../wasm/turnstile_wasm.js";

interface SolveRequest {
  challenge: string;
  difficulty: number;
  maxnumber: number;
  signalsJson: string;
  fingerprintEnabled: boolean;
  /** Flat `[x0,y0,t0, x1,y1,t1, …]`. Transferred (detached) by the caller. */
  mouse: Float64Array;
  clickIntervals: Float64Array;
  keyIntervals: Float64Array;
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
  } = e.data;
  try {
    await init();
    // Fingerprint: hash the signals and bind into the PoW seed (seed =
    // challenge || fingerprint) so a solution can't be shared across clients.
    // null when fingerprinting is disabled (GDPR PoW-only fallback).
    const fingerprint = fingerprintEnabled ? fingerprint_hash(signalsJson) : null;
    const nonce = solve_challenge(challenge, fingerprint, difficulty, maxnumber);
    // Behavior: human-likeness score (0-100) or null if too little signal.
    const behaviorScore = behavior_score(mouse, clickIntervals, keyIntervals);
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
