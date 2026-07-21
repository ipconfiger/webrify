import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { TurnstileWidget } from "./TurnstileWidget";
import type { TurnstileOptions } from "./TurnstileWidget";

export { TurnstileWidget, LABELS } from "./TurnstileWidget";
export type { TurnstileOptions, Status } from "./TurnstileWidget";

export { useTurnstile } from "./useTurnstile";
export type { UseTurnstileOptions, UseTurnstileReturn } from "./useTurnstile";

/**
 * Imperative mount for non-React host pages:
 *
 *   <div id="ts"></div>
 *   <script type="module" src="/widget/turnstile.js"></script>
 *   <script type="module">
 *     WebrifyTurnstile.mount(document.getElementById('ts')!, {
 *       endpoint: 'https://webrify.host',
 *       onVerify: (token) => { /* send token to your backend *\/ },
 *     });
 *   </script>
 *
 * `endpoint` defaults to same-origin (empty string).
 */
export function mount(container: HTMLElement, opts: TurnstileOptions) {
  return createRoot(container).render(createElement(TurnstileWidget, opts));
}
