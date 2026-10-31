import { slide } from 'svelte/transition';
import type { ZIconName } from '@shival99/z-ui-svelte';
import { zHoverGlide } from '@shival99/z-ui-svelte';
import type { ImageHistoryRun, ImageModelChoice } from '../../chat/agent';

export type ImageModel = ImageModelChoice & { label: string };

export const openAiModels: ImageModel[] = [
  { model: 'gpt-6.1-sol', backend: 'responses', label: 'GPT 6.1 Sol' },
  { model: 'gpt-6-astra', backend: 'responses', label: 'GPT 6 Astra' },
  { model: 'gpt-6-sol', backend: 'responses', label: 'GPT 6 Sol' },
  { model: 'gpt-6-luna', backend: 'responses', label: 'GPT 6 Luna' },
  { model: 'gpt-5.6-sol', backend: 'responses', label: 'GPT 5.6 Sol' },
  { model: 'gpt-5.6-terra', backend: 'responses', label: 'GPT 5.6 Terra' },
  { model: 'gpt-5.6-luna', backend: 'responses', label: 'GPT 5.6 Luna' },
  { model: 'gpt-image-2', backend: 'images', label: 'GPT Image 2' },
];
// Google's image models (Nano Banana), from the Gemini API's model list. They need the key of a Gemini provider.
export const GEMINI_HOST = 'generativelanguage.googleapis.com';
export const geminiModels: { model: string; label: string }[] = [
  { model: 'gemini-nano-banana-2.1', label: 'Nano Banana 2.1' },
  { model: 'gemini-3.1-flash-lite-image', label: 'Nano Banana 2 Lite' },
  { model: 'gemini-3.1-flash-image', label: 'Nano Banana 2' },
  { model: 'gemini-3-pro-image', label: 'Nano Banana Pro' },
];
/** Community free image generation. It is intentionally separate from configured paid APIs. */
export const freeImageModels: ImageModel[] = [
  { model: 'flux', backend: 'pollinations', label: 'Pollinations · FLUX (free)' },
];
// Match the backend's per-run limit without increasing the default generation workload.
export const MAX_IMAGE_MODELS = 4;
// A panel that opens by growing from its top edge and goes by shrinking and fading, so what is below it never jumps.
export function fold(node: HTMLElement, { duration = 280 }: { duration?: number } = {}) {
  const grow = slide(node, { duration });
  return { ...grow, css: (t: number, u: number) => `${grow.css?.(t, u) ?? ''};opacity:${t}` };
}
export const variantTabs = [1, 2, 3, 4].map((count) => ({ value: String(count), label: String(count) }));
export const EFFORT_LABEL: Record<string, string> = { off: 'Off', low: 'Low', medium: 'Medium', high: 'High', xhigh: 'X-High', max: 'Max', ultra: 'Ultra' };
// The settings under the prompt box. An empty value is Auto: nothing is sent for that setting.
export type SettingKey = 'style' | 'domain' | 'audience' | 'ratio' | 'resolution';
export const SETTINGS: { key: SettingKey; label: string; options: string[] }[] = [
  { key: 'style', label: 'Style', options: ['Photorealistic', 'Anime', '2D illustration', '3D render', 'Watercolor', 'Oil painting', 'Pixel art', 'Flat vector', 'Line art', 'Cyberpunk', 'Vintage film', 'Modern minimal'] },
  { key: 'domain', label: 'Domain', options: ['General', 'Medical and healthcare', 'Education', 'Math and geometry diagram', 'Science and biology', 'Architecture', 'Product and e-commerce', 'Fashion', 'Food', 'Game asset', 'Marketing poster', 'UI mockup'] },
  { key: 'audience', label: 'Audience', options: ['All ages', 'Teen (13+)', 'Adult (18+)'] },
  { key: 'ratio', label: 'Ratio', options: ['1:1', '16:9', '16:10', '4:3', '3:4', '9:16', '21:9', '3:2', '2:3'] },
  { key: 'resolution', label: 'Resolution', options: ['SD (640×480)', 'HD (1280×720)', 'Full HD (1920×1080)', '2K (2560×1440)', '4K (3840×2160)'] },
];
// A small icon in front of the option names that have one (Style and Domain).
export const OPTION_ICON: Record<string, ZIconName> = {
  Photorealistic: 'lucideImage' as ZIconName,
  Anime: 'lucideSparkles' as ZIconName,
  '2D illustration': 'lucidePenLine' as ZIconName,
  '3D render': 'lucideLayers' as ZIconName,
  Watercolor: 'lucidePalette' as ZIconName,
  'Oil painting': 'lucideHighlighter' as ZIconName,
  'Pixel art': 'lucideTable2' as ZIconName,
  'Flat vector': 'lucideSquare' as ZIconName,
  'Line art': 'lucidePencil' as ZIconName,
  Cyberpunk: 'lucideZap' as ZIconName,
  'Vintage film': 'lucideArchive' as ZIconName,
  'Modern minimal': 'lucideMinus' as ZIconName,
  General: 'lucideGlobe' as ZIconName,
  'Medical and healthcare': 'lucideActivity' as ZIconName,
  Education: 'lucideBrain' as ZIconName,
  'Math and geometry diagram': 'lucideSlidersHorizontal' as ZIconName,
  'Science and biology': 'lucideFlaskConical' as ZIconName,
  Architecture: 'lucideColumns3' as ZIconName,
  'Product and e-commerce': 'lucideCoins' as ZIconName,
  Fashion: 'lucideHand' as ZIconName,
  Food: 'lucideCookie' as ZIconName,
  'Game asset': 'lucidePlay' as ZIconName,
  'Marketing poster': 'lucideBell' as ZIconName,
  'UI mockup': 'lucideAppWindow' as ZIconName,
};
// The icon of each setting, shown on its chip while nothing is picked (a picked option shows its own icon).
export const SETTING_ICON: Record<SettingKey, ZIconName> = {
  style: 'lucidePalette' as ZIconName,
  domain: 'lucideGlobe' as ZIconName,
  audience: 'lucideShieldCheck' as ZIconName,
  ratio: 'lucideSquare' as ZIconName,
  resolution: 'lucideMonitor' as ZIconName,
};
export const SETTING_CATEGORY: Record<string, SettingKey> ={ style: 'style', domain: 'domain', audience: 'audience', age: 'audience', ratio: 'ratio', resolution: 'resolution' };
export const MAX_REFERENCES = 5;
export const PICTURE = /^image\/(png|jpeg|gif|webp)$/;
// One highlight box glides between the hovered rows of a menu, as in the chat's model picker.
export const glide = () => zHoverGlide({ class: 'bg-primary/10 rounded-md', exclude: '[aria-checked="true"]' });


// Whole seconds only: "43 s", then "1m 05s", then "1h 02m 05s".
export function formatDuration(ms: number) {
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, '0');
  if (hours) return `${hours}h ${String(minutes).padStart(2, '0')}m ${seconds}s`;
  if (minutes) return `${minutes}m ${seconds}s`;
  return `${total} s`;
}

export const readPicture = (file: File) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });

export function dayLabel(millis: number) {
  const date = new Date(millis);
  const today = new Date();
  const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return 'Today';
  if (date.toDateString() === yesterday.toDateString()) return 'Yesterday';
  return date.toLocaleDateString(undefined, { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric' });
}

export function timeLabel(millis: number) {
  return new Date(millis).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}


export function statusLabel(status: ImageHistoryRun['status']) {
  return status === 'stopped' ? 'Stopped' : status === 'failed' ? 'Failed' : null;
}
