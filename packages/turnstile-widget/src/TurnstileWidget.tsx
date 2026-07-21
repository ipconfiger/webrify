import { useTurnstile } from "./useTurnstile";

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
  /** CSS class applied to the button element. */
  className?: string;
  /** Inline styles merged on top of defaults. User styles override defaults. */
  style?: React.CSSProperties;
  /** URL of the PoW worker script. Defaults to the bundled worker asset. */
  workerUrl?: string;
}

export type Status = "idle" | "fetching" | "solving" | "verifying" | "success" | "error";

export const LABELS: Record<Status, string> = {
  idle: "Verify you are human",
  fetching: "Preparing…",
  solving: "Working…",
  verifying: "Verifying…",
  success: "Verified ✓",
  error: "Retry",
};

export const BUSY: ReadonlySet<Status> = new Set(["fetching", "solving", "verifying"]);

export function TurnstileWidget({
  endpoint = "",
  onVerify,
  onError,
  disableFingerprint = false,
  className,
  style,
  workerUrl,
}: TurnstileOptions) {
  const { status, errorMessage, verify } = useTurnstile({
    endpoint,
    onVerify,
    onError,
    disableFingerprint,
    workerUrl,
  });

  const defaultStyle: React.CSSProperties = {
    fontFamily: "system-ui, sans-serif",
    fontSize: "14px",
    padding: "8px 16px",
    borderRadius: "6px",
    border: "1px solid #ccc",
    background: status === "success" ? "#e6f4ea" : "#fff",
    color: status === "error" ? "#d93025" : "#1f1f1f",
    cursor:
      status === "idle" || status === "error" ? "pointer" : "default",
  };

  return (
    <button
      type="button"
      onClick={verify}
      disabled={BUSY.has(status) || status === "success"}
      aria-busy={BUSY.has(status)}
      aria-live="polite"
      className={className}
      style={{ ...defaultStyle, ...style }}
    >
      {LABELS[status]}
      {status === "error" && errorMessage ? ` — ${errorMessage}` : null}
    </button>
  );
}
