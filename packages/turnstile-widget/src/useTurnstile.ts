import { useCallback, useEffect, useRef, useState } from "react";
import { BehaviorRecorder, type BehaviorSnapshot } from "./behavior";
import { collectSignals } from "./fingerprint";
import PowWorker from "./pow-worker?worker&inline";
import type { Challenge, VerifyResponse } from "./types";

export interface UseTurnstileOptions {
  /** Base URL of the Webrify server. Empty string = same origin. */
  endpoint?: string;
  /** Called with the issued JWT on successful verification. */
  onVerify: (token: string) => void;
  /** Called with a human-readable message on failure. */
  onError?: (message: string) => void;
  /**
   * Skip fingerprint collection entirely (PoW-only mode). Raw signals never
   * leave the browser either way — but this avoids collecting them at all, the
   * GDPR "no fingerprinting" / minimization fallback. The server accepts
   * fingerprint-less verifications (PoW seed = challenge bytes only).
   */
  disableFingerprint?: boolean;
  /** URL of the PoW worker script. Defaults to the bundled inline worker (self-contained). */
  workerUrl?: string;
}

export interface UseTurnstileReturn {
  status: "idle" | "fetching" | "solving" | "verifying" | "success" | "error";
  errorMessage: string | null;
  /** PoW solve progress 0–100 (estimated, updated every 200ms). */
  progress: number;
  verify: () => Promise<void>;
  reset: () => void;
}

type Status = UseTurnstileReturn["status"];

const EMPTY_BEHAVIOR: BehaviorSnapshot = {
  mouse: new Float64Array(0),
  clickIntervals: new Float64Array(0),
  keyIntervals: new Float64Array(0),
  clickPositions: new Float64Array(0),
};

export function useTurnstile({
  endpoint = "",
  onVerify,
  onError,
  disableFingerprint = false,
  workerUrl,
}: UseTurnstileOptions): UseTurnstileReturn {
  const [status, setStatus] = useState<Status>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
    const [progress, setProgress] = useState<number>(0);
    const behaviorRef = useRef<BehaviorRecorder | null>(null);

  // Record interaction telemetry from mount until unmount — the longer the user
  // is on the page before clicking, the richer the behavior signal.
  useEffect(() => {
    const recorder = new BehaviorRecorder();
    behaviorRef.current = recorder;
    recorder.start();
    return () => {
      recorder.stop();
      behaviorRef.current = null;
    };
  }, []);

  const solve = (
    challenge: Challenge,
    signalsJson: string,
    fingerprintEnabled: boolean,
    behavior: BehaviorSnapshot,
  ): Promise<{
    nonce: number;
    fingerprint: string | null;
    behaviorScore: number | null;
  }> => {
    // Determine worker count: min(hardwareConcurrency, 4), fallback to 2.
    const numWorkers = Math.min(
      navigator.hardwareConcurrency || 2,
      4,
    );
    const chunkSize = Math.ceil(challenge.maxnumber / numWorkers);
    const workers: Worker[] = [];

    // Clone behavior data for each worker — ArrayBuffers are transferred
    // (detached) so each worker needs its own copy.
    const cloneFloat64 = (arr: Float64Array) => Float64Array.from(arr);
    const spawnWorker = (start: number, end: number, isPrimary: boolean): Promise<{
      nonce: number;
      fingerprint: string | null;
      behaviorScore: number | null;
    }> =>
      new Promise((resolveWorker, rejectWorker) => {
        const w = workerUrl
          ? new Worker(workerUrl, { type: "module" })
          : new PowWorker();
        workers.push(w);
        w.onmessage = (e: MessageEvent) => {
          const data = e.data as {
            ok?: boolean; progress?: number; exhausted?: boolean;
            nonce?: number; fingerprint?: string | null;
            behaviorScore?: number | null; error?: string;
          };
          if (typeof data.progress === "number") {
            setProgress((prev) => Math.max(prev, data.progress!));
            return;
          }
          if (data.exhausted) { w.terminate(); rejectWorker(new Error("range exhausted")); return; }
          if (data.ok && typeof data.nonce === "number") {
            w.terminate();
            resolveWorker({ nonce: data.nonce, fingerprint: data.fingerprint ?? null, behaviorScore: data.behaviorScore ?? null });
          } else {
            w.terminate();
            rejectWorker(new Error(data.error ?? "solve failed"));
          }
        };
        w.onerror = (e: ErrorEvent) => { w.terminate(); rejectWorker(new Error(e.message || "worker error")); };
        // Only the primary worker computes fingerprint and behavior; others do PoW only.
        const bm = isPrimary ? cloneFloat64(behavior.mouse) : new Float64Array(0);
        const bci = isPrimary ? cloneFloat64(behavior.clickIntervals) : new Float64Array(0);
        const bki = isPrimary ? cloneFloat64(behavior.keyIntervals) : new Float64Array(0);
        const bcp = isPrimary ? cloneFloat64(behavior.clickPositions) : new Float64Array(0);
        w.postMessage(
          { challenge: challenge.challenge, difficulty: challenge.difficulty, maxnumber: challenge.maxnumber, nonceStart: start, nonceEnd: end, signalsJson, fingerprintEnabled: isPrimary && fingerprintEnabled, mouse: bm, clickIntervals: bci, keyIntervals: bki, clickPositions: bcp },
          isPrimary ? [bm.buffer, bci.buffer, bki.buffer, bcp.buffer] : [],
        );
      });

    // Spawn workers, each searching a non-overlapping range.
    const promises: Promise<{
      nonce: number;
      fingerprint: string | null;
      behaviorScore: number | null;
    }>[] = [];
    for (let i = 0; i < numWorkers; i++) {
      const start = i * chunkSize;
      const end = Math.min(start + chunkSize - 1, challenge.maxnumber);
      promises.push(spawnWorker(start, end, i === 0));
    }

    // First worker to find a valid nonce wins.
    return Promise.any(promises).finally(() => {
      // Clean up any remaining workers.
      for (const w of workers) {
        try { w.terminate(); } catch { /* already terminated */ }
      }
    });
  };

  const verify = useCallback(async () => {
    try {
      setStatus("fetching");
      const t0 = performance.now();
      const chalRes = await fetch(`${endpoint}/challenge`, { method: "POST" });
      if (!chalRes.ok) throw new Error(`challenge failed (${chalRes.status})`);
      const challenge = (await chalRes.json()) as Challenge;

      setStatus("solving");
      const fingerprintEnabled = !disableFingerprint;
      const signalsJson = fingerprintEnabled ? await collectSignals() : "";
      const behavior = behaviorRef.current?.snapshot() ?? EMPTY_BEHAVIOR;
      const { nonce, fingerprint, behaviorScore } = await solve(
        challenge,
        signalsJson,
        fingerprintEnabled,
        behavior,
      );
      const solveTimeMs = Math.round(performance.now() - t0);

      setStatus("verifying");
      const verifyRes = await fetch(`${endpoint}/verify`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          protocol_version: challenge.protocol_version,
          algorithm: challenge.algorithm,
          challenge: challenge.challenge,
          salt: challenge.salt,
          difficulty: challenge.difficulty,
          maxnumber: challenge.maxnumber,
          expires_at: challenge.expires_at,
          origin: challenge.origin,
          signature: challenge.signature,
          nonce,
          solve_time_ms: solveTimeMs,
          ...(fingerprint !== null ? { fingerprint } : {}),
          ...(behaviorScore !== null ? { behavior_score: behaviorScore } : {}),
        }),
      });
      if (!verifyRes.ok) {
        const body = (await verifyRes.json().catch(() => ({}))) as { error?: string };
        throw new Error(body.error ?? `verify failed (${verifyRes.status})`);
      }
      const result = (await verifyRes.json()) as VerifyResponse;
      setStatus("success");
      onVerify(result.token);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setErrorMessage(msg);
      setStatus("error");
      onError?.(msg);
    }
  }, [endpoint, onVerify, onError, disableFingerprint, workerUrl]);

  const reset = useCallback(() => {
    setStatus("idle");
    setErrorMessage(null);
    setProgress(0);
  }, []);

  return { status, errorMessage, progress, verify, reset };
}
