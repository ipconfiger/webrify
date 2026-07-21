import { useCallback, useEffect, useRef, useState } from "react";
import { BehaviorRecorder, type BehaviorSnapshot } from "./behavior";
import { collectSignals } from "./fingerprint";
import PowWorker from "./pow-worker?worker";
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
}

export interface UseTurnstileReturn {
  status: "idle" | "fetching" | "solving" | "verifying" | "success" | "error";
  errorMessage: string | null;
  verify: () => Promise<void>;
  reset: () => void;
}

type Status = UseTurnstileReturn["status"];

/** Generate a UUID v4 using `crypto.getRandomValues` — works in non-secure contexts
 *  where `crypto.randomUUID()` is unavailable (plain HTTP on non-localhost). */
function randomUUID(): string {
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const r = (crypto.getRandomValues(new Uint8Array(1))[0] % 16) | 0;
    const v = c === "x" ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

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
}: UseTurnstileOptions): UseTurnstileReturn {
  const [status, setStatus] = useState<Status>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const workerRef = useRef<Worker | null>(null);
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
  }> =>
    new Promise((resolve, reject) => {
      const worker = new PowWorker();
      workerRef.current = worker;
      worker.onmessage = (e: MessageEvent) => {
        const data = e.data as {
          ok: boolean;
          nonce?: number;
          fingerprint?: string | null;
          behaviorScore?: number | null;
          error?: string;
        };
        worker.terminate();
        workerRef.current = null;
        if (
          data.ok &&
          typeof data.nonce === "number" &&
          (data.fingerprint === null || typeof data.fingerprint === "string") &&
          (data.behaviorScore === null ||
            data.behaviorScore === undefined ||
            typeof data.behaviorScore === "number")
        ) {
          resolve({
            nonce: data.nonce,
            fingerprint: data.fingerprint ?? null,
            behaviorScore: data.behaviorScore ?? null,
          });
        } else {
          reject(new Error(data.error ?? "solve failed"));
        }
      };
      worker.onerror = (e: ErrorEvent) => {
        worker.terminate();
        workerRef.current = null;
        reject(new Error(e.message || "worker error"));
      };
      // Transfer the underlying ArrayBuffers (zero-copy) — they're freshly
      // snapshotted, so detaching on the main thread is fine.
      worker.postMessage(
        {
          challenge: challenge.challenge,
          difficulty: challenge.difficulty,
          maxnumber: challenge.maxnumber,
          signalsJson,
          fingerprintEnabled,
          mouse: behavior.mouse,
          clickIntervals: behavior.clickIntervals,
          keyIntervals: behavior.keyIntervals,
          clickPositions: behavior.clickPositions,
        },
        [
          behavior.mouse.buffer,
          behavior.clickIntervals.buffer,
          behavior.keyIntervals.buffer,
          behavior.clickPositions.buffer,
        ],
      );
    });

  const verify = useCallback(async () => {
    try {
      setStatus("fetching");
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

      setStatus("verifying");
      const verifyRes = await fetch(`${endpoint}/verify`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          algorithm: challenge.algorithm,
          challenge: challenge.challenge,
          salt: challenge.salt,
          difficulty: challenge.difficulty,
          maxnumber: challenge.maxnumber,
          expires_at: challenge.expires_at,
          origin: challenge.origin,
          signature: challenge.signature,
          nonce,
          idempotency_key: randomUUID(),
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
  }, [endpoint, onVerify, onError, disableFingerprint]);

  const reset = useCallback(() => {
    setStatus("idle");
    setErrorMessage(null);
  }, []);

  return { status, errorMessage, verify, reset };
}
