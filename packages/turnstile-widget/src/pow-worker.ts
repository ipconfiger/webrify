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
}

self.onmessage = async (e: MessageEvent<SolveRequest>) => {
  const { challenge, difficulty, maxnumber, signalsJson } = e.data;
  try {
    await init();
    // Hash the collected signals to a 128-bit fingerprint, then bind it into
    // the PoW seed (seed = challenge || fingerprint) so the solution can't be
    // shared across clients.
    const fingerprint = fingerprint_hash(signalsJson);
    const nonce = solve_challenge(challenge, fingerprint, difficulty, maxnumber);
    (self as unknown as Worker).postMessage({ ok: true, nonce, fingerprint });
  } catch (err) {
    (self as unknown as Worker).postMessage({
      ok: false,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};
