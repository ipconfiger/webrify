import { useCallback, useRef, useState } from "react";
import { collectSignals } from "./fingerprint";
import PowWorker from "./pow-worker?worker";
import type { Challenge, VerifyResponse } from "./types";

export interface TurnstileOptions {
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

type Status = "idle" | "fetching" | "solving" | "verifying" | "success" | "error";

const LABELS: Record<Status, string> = {
  idle: "Verify you are human",
  fetching: "Preparing…",
  solving: "Working…",
  verifying: "Verifying…",
  success: "Verified ✓",
  error: "Retry",
};

const BUSY: ReadonlySet<Status> = new Set(["fetching", "solving", "verifying"]);

export function TurnstileWidget({
  endpoint = "",
  onVerify,
  onError,
  disableFingerprint = false,
}: TurnstileOptions) {
  const [status, setStatus] = useState<Status>("idle");
  const [errorMsg, setErrorMsg] = useState("");
  const workerRef = useRef<Worker | null>(null);

  const solve = (
    challenge: Challenge,
    signalsJson: string,
    fingerprintEnabled: boolean,
  ): Promise<{ nonce: number; fingerprint: string | null }> =>
    new Promise((resolve, reject) => {
      const worker = new PowWorker();
      workerRef.current = worker;
      worker.onmessage = (e: MessageEvent) => {
        const data = e.data as {
          ok: boolean;
          nonce?: number;
          fingerprint?: string | null;
          error?: string;
        };
        worker.terminate();
        workerRef.current = null;
        if (
          data.ok &&
          typeof data.nonce === "number" &&
          (data.fingerprint === null || typeof data.fingerprint === "string")
        ) {
          resolve({ nonce: data.nonce, fingerprint: data.fingerprint ?? null });
        } else {
          reject(new Error(data.error ?? "solve failed"));
        }
      };
      worker.onerror = (e: ErrorEvent) => {
        worker.terminate();
        workerRef.current = null;
        reject(new Error(e.message || "worker error"));
      };
      worker.postMessage({
        challenge: challenge.challenge,
        difficulty: challenge.difficulty,
        maxnumber: challenge.maxnumber,
        signalsJson,
        fingerprintEnabled,
      });
    });

  const run = useCallback(async () => {
    try {
      setStatus("fetching");
      const chalRes = await fetch(`${endpoint}/challenge`, { method: "POST" });
      if (!chalRes.ok) throw new Error(`challenge failed (${chalRes.status})`);
      const challenge = (await chalRes.json()) as Challenge;

      setStatus("solving");
      const fingerprintEnabled = !disableFingerprint;
      const signalsJson = fingerprintEnabled ? await collectSignals() : "";
      const { nonce, fingerprint } = await solve(
        challenge,
        signalsJson,
        fingerprintEnabled,
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
          idempotency_key: crypto.randomUUID(),
          ...(fingerprint !== null ? { fingerprint } : {}),
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
      setErrorMsg(msg);
      setStatus("error");
      onError?.(msg);
    }
  }, [endpoint, onVerify, onError, disableFingerprint]);

  return (
    <button
      type="button"
      onClick={run}
      disabled={BUSY.has(status) || status === "success"}
      aria-busy={BUSY.has(status)}
      aria-live="polite"
      style={{
        fontFamily: "system-ui, sans-serif",
        fontSize: "14px",
        padding: "8px 16px",
        borderRadius: "6px",
        border: "1px solid #ccc",
        background: status === "success" ? "#e6f4ea" : "#fff",
        color: status === "error" ? "#d93025" : "#1f1f1f",
        cursor:
          status === "idle" || status === "error" ? "pointer" : "default",
      }}
    >
      {LABELS[status]}
      {status === "error" && errorMsg ? ` — ${errorMsg}` : null}
    </button>
  );
}
