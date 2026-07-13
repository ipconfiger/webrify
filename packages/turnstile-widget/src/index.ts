import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { TurnstileWidget } from "./TurnstileWidget";
import type { TurnstileOptions } from "./TurnstileWidget";

export { TurnstileWidget };
export type { TurnstileOptions };

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
