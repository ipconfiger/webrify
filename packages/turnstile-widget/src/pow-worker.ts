// Web Worker: loads the WASM module and runs fingerprint hashing + PoW
// off the main thread so the tight SHA-256 loops never freeze the UI.
// The wasm pkg is copied into `../wasm/` by the `build-widget` justfile recipe
// before `vite build`.

import init, { fingerprint_hash, solve_challenge } from "../wasm/turnstile_wasm.js";

interface SolveRequest {
  challenge: string;
  difficulty: number;
  maxnumber: number;
  signalsJson: string;
  fingerprintEnabled: boolean;
}

self.onmessage = async (e: MessageEvent<SolveRequest>) => {
  const { challenge, difficulty, maxnumber, signalsJson, fingerprintEnabled } =
    e.data;
  try {
    await init();
    // When fingerprinting is enabled, hash the signals and bind the result into
    // the PoW seed (seed = challenge || fingerprint) so a solution can't be
    // shared across clients. When disabled (GDPR PoW-only fallback), pass null
    // and the wasm solves over the challenge bytes alone.
    const fingerprint = fingerprintEnabled ? fingerprint_hash(signalsJson) : null;
    const nonce = solve_challenge(challenge, fingerprint, difficulty, maxnumber);
    (self as unknown as Worker).postMessage({ ok: true, nonce, fingerprint });
  } catch (err) {
    (self as unknown as Worker).postMessage({
      ok: false,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};
