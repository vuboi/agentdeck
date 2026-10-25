// The image generator's run, kept out of its panel: the panel is unmounted when another tool tab is picked or the
// right panel is folded, and the run must go on (and be there again when the panel returns). A run is also an entry
// of the Tasks panel while it works, and of the Audit log once it ends.

import { generateImages, listImageHistory, readImageDataUrl, stopImageGeneration, suggestImagePrompts, type GeneratedImage, type ImageHistoryRun, type ImageModelChoice, type ImageSettings, type SuggestedKeyword } from './agent';
import { trackImageRun } from './activity.svelte';
import { inTauri } from './history';
import { toast } from './toast';

export type Preview = GeneratedImage & { src: string };

export const gen = $state({
  working: false,
  stopping: false,
  previews: [] as Preview[],
  failureLines: [] as string[],
  /** Time of the current or last run. */
  runMs: 0,
  /** The skeleton of a run is fixed when it starts: changing the settings only shapes the next run. */
  runSkeleton: 2,
  activeRunId: null as string | null,
  loadingRun: 0,
  history: [] as ImageHistoryRun[],
  /** The prompt suggestions: being written, and what came back. They too outlive the panel. */
  suggesting: false,
  suggestions: [] as string[],
  keywords: [] as SuggestedKeyword[],
});

/** What the panel's inputs held when it was last closed, so they are there when it opens again. Not reactive. */
export const draft: {
  prompt: string;
  selectedModels: string[] | null;
  variants: number;
  fast: boolean;
  effort: string;
  promptModel: string;
  settings: Record<'style' | 'domain' | 'audience' | 'ratio' | 'resolution', string>;
  references: { id: string; name: string; mime: string; src: string; path?: string }[];
} = { prompt: '', selectedModels: null, variants: 2, fast: true, effort: 'medium', promptModel: 'gpt-6-luna', settings: { style: '', domain: '', audience: '', ratio: '', resolution: '' }, references: [] };

let runStart = 0;
let runTimer: ReturnType<typeof setInterval> | undefined;

/** The saved runs. `quiet` is for the load when the panel opens: an error there is not something the user asked for. */
export async function refreshHistory(quiet = false) {
  try {
    gen.history = await listImageHistory();
  } catch (error) {
    if (!quiet) toast.error(error instanceof Error ? error.message : String(error));
  }
}

export async function withPreviews(images: GeneratedImage[]): Promise<Preview[]> {
  const loaded = await Promise.all(
    images.map(async (image) => {
      try {
        return { ...image, src: await readImageDataUrl(image.path) };
      } catch {
        return null;
      }
    }),
  );
  return loaded.filter((image): image is Preview => image !== null);
}

/** Starts a run that goes on whatever the panel does. Resolves when the run has ended. */
export async function startRun(args: { prompt: string; choices: (ImageModelChoice & { label: string })[]; variants: number; cwd: string | null; project: string; referencePaths: () => Promise<string[]>; settings: ImageSettings }) {
  if (gen.working) return;
  if (!inTauri()) {
    toast.error('Image generation needs the desktop app');
    return;
  }
  gen.runSkeleton = Math.max(2, Math.min(6, args.choices.length * args.variants));
  gen.working = true;
  runStart = performance.now();
  gen.runMs = 0;
  clearInterval(runTimer);
  runTimer = setInterval(() => (gen.runMs = performance.now() - runStart), 100);
  gen.stopping = false;
  gen.previews = [];
  gen.failureLines = [];
  gen.activeRunId = null;
  const finish = trackImageRun(args.project, args.prompt, args.choices.map((choice) => choice.label).join(', '));
  try {
    const result = await generateImages({ prompt: args.prompt, models: args.choices.map(({ model, backend }) => ({ model, backend })), variants: args.variants, cwd: args.cwd, referencePaths: await args.referencePaths(), settings: args.settings });
    gen.activeRunId = result.run_id;
    gen.failureLines = result.failures.map((failure) => `${failure.model} #${failure.variant}: ${failure.error}`);
    gen.previews = await withPreviews(result.images);
    if (!gen.previews.length && gen.failureLines.length) toast.error('No image was generated. Check the details below.');
    const stopped = gen.stopping;
    finish(stopped ? 'stopped' : gen.previews.length ? 'ok' : 'fail', [`${gen.previews.length} image(s) in run ${result.run_id}`, ...gen.previews.map((image) => image.path), ...gen.failureLines].join('\n'));
    void refreshHistory();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    toast.error(message);
    finish('fail', message);
  } finally {
    clearInterval(runTimer);
    gen.runMs = performance.now() - runStart;
    gen.working = false;
    gen.stopping = false;
  }
}

export async function stopRun() {
  gen.stopping = true;
  try {
    await stopImageGeneration();
  } catch (error) {
    gen.stopping = false;
    toast.error(error instanceof Error ? error.message : String(error));
  }
}

export async function openRun(run: ImageHistoryRun) {
  gen.activeRunId = run.id;
  gen.failureLines = run.failures;
  gen.previews = [];
  // Skeletons until the saved files are read back, so the click answers at once.
  gen.loadingRun = run.images.length;
  try {
    gen.previews = await withPreviews(run.images.map((image) => ({ model: image.model, backend: image.backend, path: image.path, variant: image.variant })));
  } finally {
    gen.loadingRun = 0;
  }
}

/** Asks the prompt model for suggestions. The answer lands here, so it is there when the panel is opened again. */
export async function runSuggest(args: { prompt: string; model: string; provider: string; referencePaths: () => Promise<string[]>; settings: ImageSettings }) {
  if (gen.suggesting) return;
  gen.suggesting = true;
  gen.suggestions = [];
  gen.keywords = [];
  try {
    const result = await suggestImagePrompts({ prompt: args.prompt, model: args.model, provider: args.provider, referencePaths: await args.referencePaths(), settings: args.settings });
    gen.suggestions = result.suggestions;
    gen.keywords = result.keywords;
    if (!result.suggestions.length) toast.error('The prompt model returned no suggestions');
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
  } finally {
    gen.suggesting = false;
  }
}

/** A suggestion was used (or dismissed): the list goes. */
export function clearSuggestions() {
  gen.suggestions = [];
  gen.keywords = [];
}
