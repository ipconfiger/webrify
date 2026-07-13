// Web Worker: loads the WASM PoW solver and runs it off the main thread so the
// tight SHA-256 loop never freezes the UI. The wasm pkg is copied into
// `../wasm/` by the `build-widget` justfile recipe before `vite build`.

import init, { solve_challenge } from "../wasm/turnstile_wasm.js";

interface SolveRequest {
  challenge: string;
  difficulty: number;
  maxnumber: number;
}

self.onmessage = async (e: MessageEvent<SolveRequest>) => {
  const { challenge, difficulty, maxnumber } = e.data;
  try {
    await init();
    const nonce = solve_challenge(challenge, difficulty, maxnumber);
    (self as unknown as Worker).postMessage({ ok: true, nonce });
  } catch (err) {
    (self as unknown as Worker).postMessage({
      ok: false,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};
