// Browser signal collection for fingerprinting.
//
// Gathers high-entropy environment signals (Canvas, WebGL, AudioContext,
// navigator/screen/timezone) into a canonical JSON string with sorted keys.
// The WASM worker hashes this to a 128-bit fingerprint. Raw signals NEVER leave
// the browser — only the hash is sent to the server (GDPR data minimization).
//
// Every collector is defensive: a missing/blocked API yields an empty string
// rather than throwing, so verification still works (just with less entropy).

type NavExt = Navigator & {
  deviceMemory?: number;
  platform?: string;
  userAgentData?: { mobile?: boolean; platform?: string };
};

/** Collect all signals and return canonical (sorted-key) JSON. */
export async function collectSignals(): Promise<string> {
  const signals: Record<string, string> = {
    audio: await audioSignal().catch(() => ""),
    canvas: canvasSignal(),
    webgl: webglSignal(),
    hardwareConcurrency: String(navigator.hardwareConcurrency ?? 0),
    deviceMemory: String((navigator as NavExt).deviceMemory ?? 0),
    language: navigator.language ?? "",
    languages: (navigator.languages ?? []).join(","),
    platform: (navigator as NavExt).platform ?? "",
    screen: `${screen.width}x${screen.height}x${screen.colorDepth}`,
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone ?? "",
    timezoneOffset: String(new Date().getTimezoneOffset()),
  };
  // Canonical: sort keys so identical environments hash identically.
  const sorted: Record<string, string> = {};
  for (const key of Object.keys(signals).sort()) {
    sorted[key] = signals[key];
  }
  return JSON.stringify(sorted);
}

/** Canvas fingerprint: render text + shapes, return the data URL. */
function canvasSignal(): string {
  try {
    const canvas = document.createElement("canvas");
    canvas.width = 240;
    canvas.height = 60;
    const ctx = canvas.getContext("2d");
    if (!ctx) return "";
    ctx.textBaseline = "top";
    ctx.font = "16px 'Arial'";
    ctx.fillStyle = "#f60";
    ctx.fillRect(125, 1, 62, 20);
    ctx.fillStyle = "#069";
    ctx.fillText("Webrify Turnstile · 人机验证", 2, 15);
    ctx.fillStyle = "rgba(102, 204, 0, 0.7)";
    ctx.fillText("Webrify Turnstile · 人机验证", 4, 17);
    return canvas.toDataURL();
  } catch {
    return "";
  }
}

/** WebGL fingerprint: renderer + vendor strings. */
function webglSignal(): string {
  try {
    const canvas = document.createElement("canvas");
    const gl = (canvas.getContext("webgl") ?? canvas.getContext("experimental-webgl")) as
      | WebGLRenderingContext
      | null;
    if (!gl) return "";
    const dbg = gl.getExtension("WEBGL_debug_renderer_info");
    if (!dbg) return `${gl.getParameter(gl.VENDOR)}|${gl.getParameter(gl.RENDERER)}`;
    const vendor = gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL);
    const renderer = gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL);
    return `${vendor}|${renderer}`;
  } catch {
    return "";
  }
}

/**
 * AudioContext fingerprint: render an oscillator through a compressor and
 * sum the output samples. Varies subtly across OS/GPU/driver stacks.
 */
async function audioSignal(): Promise<string> {
  const Ctx =
    window.OfflineAudioContext ||
    (window as unknown as { webkitOfflineAudioContext?: typeof OfflineAudioContext })
      .webkitOfflineAudioContext;
  if (!Ctx) return "";
  const ctx = new Ctx(1, 44100, 44100);
  const oscillator = ctx.createOscillator();
  oscillator.type = "triangle";
  oscillator.frequency.value = 10000;
  const compressor = ctx.createDynamicsCompressor();
  compressor.threshold.value = -50;
  compressor.knee.value = 40;
  compressor.ratio.value = 12;
  compressor.attack.value = 0;
  compressor.release.value = 0.25;
  oscillator.connect(compressor);
  compressor.connect(ctx.destination);
  oscillator.start(0);
  const rendered = await ctx.startRendering();
  const data = rendered.getChannelData(0);
  let sum = 0;
  let index = 4500; // skip the initial transient
  while (index < 5000) {
    sum += Math.abs(data[index] ?? 0);
    index += 1;
  }
  return sum.toString();
}
