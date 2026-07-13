import { useCallback, useRef, useState } from "react";
import PowWorker from "./pow-worker?worker";
import type { Challenge, VerifyResponse } from "./types";

export interface TurnstileOptions {
  /** Base URL of the Webrify server. Empty string = same origin. */
  endpoint?: string;
  /** Called with the issued JWT on successful verification. */
  onVerify: (token: string) => void;
  /** Called with a human-readable message on failure. */
  onError?: (message: string) => void;
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

export function TurnstileWidget({
  endpoint = "",
  onVerify,
  onError,
}: TurnstileOptions) {
  const [status, setStatus] = useState<Status>("idle");
  const [errorMsg, setErrorMsg] = useState("");
  const workerRef = useRef<Worker | null>(null);

  const solve = (challenge: Challenge): Promise<number> =>
    new Promise((resolve, reject) => {
      const worker = new PowWorker();
      workerRef.current = worker;
      worker.onmessage = (e: MessageEvent) => {
        const data = e.data as { ok: boolean; nonce?: number; error?: string };
        worker.terminate();
        workerRef.current = null;
        if (data.ok && typeof data.nonce === "number") resolve(data.nonce);
        else reject(new Error(data.error ?? "solve failed"));
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
      });
    });

  const run = useCallback(async () => {
    try {
      setStatus("fetching");
      const chalRes = await fetch(`${endpoint}/challenge`, { method: "POST" });
      if (!chalRes.ok) throw new Error(`challenge failed (${chalRes.status})`);
      const challenge = (await chalRes.json()) as Challenge;

      setStatus("solving");
      const nonce = await solve(challenge);

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
  }, [endpoint, onVerify, onError]);

  return (
    <button
      type="button"
      onClick={run}
      disabled={status !== "idle" && status !== "error"}
      style={{
        fontFamily: "system-ui, sans-serif",
        fontSize: "14px",
        padding: "8px 16px",
        borderRadius: "6px",
        border: "1px solid #ccc",
        background: status === "success" ? "#e6f4ea" : "#fff",
        color: status === "error" ? "#d93025" : "#1f1f1f",
        cursor: status === "idle" || status === "error" ? "pointer" : "default",
      }}
      aria-live="polite"
    >
      {LABELS[status]}
      {status === "error" && errorMsg ? ` — ${errorMsg}` : null}
    </button>
  );
}
