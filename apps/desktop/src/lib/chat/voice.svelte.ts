import { inTauri } from '../app/platform/history';
import { nextVoicePhase, type VoicePhase } from './voice-state';

type RecordedPcm = {
    audioBase64: string;
    sampleRate: number;
    channels: number;
};

const recorderMimeType = () =>
    ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg;codecs=opus'].find(
        (type) => MediaRecorder.isTypeSupported(type),
    );

const errorMessage = (error: unknown): string => {
    if (error instanceof DOMException && error.name === 'NotAllowedError')
        return 'Microphone permission was denied';
    if (error instanceof DOMException && error.name === 'NotFoundError')
        return 'No microphone was found';
    return error instanceof Error ? error.message : String(error);
};

function bytesToBase64(bytes: Uint8Array): string {
    let binary = '';
    const chunk = 32_768;
    for (let offset = 0; offset < bytes.length; offset += chunk) {
        binary += String.fromCharCode(
            ...bytes.subarray(offset, offset + chunk),
        );
    }
    return btoa(binary);
}

async function recordedPcm(blob: Blob): Promise<RecordedPcm> {
    const context = new AudioContext();
    try {
        const audio = await context.decodeAudioData(await blob.arrayBuffer());
        const channels = audio.numberOfChannels;
        const interleaved = new Float32Array(audio.length * channels);
        for (let channel = 0; channel < channels; channel++) {
            const source = audio.getChannelData(channel);
            for (let frame = 0; frame < audio.length; frame++)
                interleaved[frame * channels + channel] = source[frame];
        }
        return {
            audioBase64: bytesToBase64(new Uint8Array(interleaved.buffer)),
            sampleRate: audio.sampleRate,
            channels,
        };
    } finally {
        await context.close();
    }
}

/** Owns one microphone recording and local Whisper transcription at a time. */
class VoiceCapture {
    phase = $state<VoicePhase>('idle');
    elapsedMs = $state(0);
    modelProgress = $state(0);
    level = $state(0);
    levels = $state<number[]>([0, 0, 0, 0, 0]);
    error = $state('');

    private recorder: MediaRecorder | null = null;
    private stream: MediaStream | null = null;
    private meterContext: AudioContext | null = null;
    private meterSource: MediaStreamAudioSourceNode | null = null;
    private meterFrame = 0;
    private chunks: Blob[] = [];
    private timer: ReturnType<typeof setInterval> | null = null;
    private startedAt = 0;
    private token = 0;
    private starting = false;

    /** Requests the microphone and begins a toggle-style recording. */
    async start(): Promise<void> {
        if (this.phase !== 'idle' || this.starting) return;
        if (!inTauri()) throw new Error('Voice input needs the desktop app');
        if (
            !navigator.mediaDevices?.getUserMedia ||
            typeof MediaRecorder === 'undefined'
        ) {
            throw new Error('Voice input is not available in this WebView');
        }
        this.starting = true;
        this.error = '';
        try {
            const stream = await navigator.mediaDevices.getUserMedia({
                audio: true,
            });
            const mimeType = recorderMimeType();
            const recorder = new MediaRecorder(
                stream,
                mimeType ? { mimeType } : undefined,
            );
            this.stream = stream;
            this.recorder = recorder;
            this.chunks = [];
            this.startMeter(stream);
            recorder.ondataavailable = (event) => {
                if (event.data.size > 0) this.chunks.push(event.data);
            };
            recorder.start(250);
            this.token++;
            this.phase = nextVoicePhase(this.phase, 'start');
            this.startedAt = performance.now();
            this.elapsedMs = 0;
            this.timer = setInterval(
                () => (this.elapsedMs = performance.now() - this.startedAt),
                200,
            );
        } catch (error) {
            this.releaseMedia();
            this.phase = nextVoicePhase(this.phase, 'fail');
            this.error = errorMessage(error);
            throw new Error(this.error);
        } finally {
            this.starting = false;
        }
    }

    /** Stops recording, downloads the model when needed, and returns editable transcript text. */
    async stop(): Promise<string | null> {
        if (this.phase !== 'recording' || !this.recorder) return null;
        const mine = this.token;
        this.phase = nextVoicePhase(this.phase, 'stop');
        this.stopTimer();
        this.modelProgress = 0;
        try {
            const blob = await this.stopRecorder();
            if (mine !== this.token) return null;
            const pcm = await recordedPcm(blob);
            if (mine !== this.token) return null;
            const { invoke } = await import('@tauri-apps/api/core');
            let off = () => {};
            try {
                const { listen } = await import('@tauri-apps/api/event');
                off = await listen<{ done: number; total: number }>(
                    'voice-model-progress',
                    ({ payload }) => {
                        if (mine === this.token)
                            this.modelProgress = payload.total
                                ? Math.min(1, payload.done / payload.total)
                                : 0;
                    },
                );
            } catch {
                // Event listening not permitted or in preview
            }
            try {
                const text = await invoke<string>('voice_transcribe', pcm);
                if (mine !== this.token) return null;
                return text.trim();
            } finally {
                off();
            }
        } catch (error) {
            this.releaseMedia();
            if (mine === this.token) {
                this.phase = nextVoicePhase(this.phase, 'fail');
                this.error = errorMessage(error);
            }
            throw new Error(errorMessage(error));
        } finally {
            if (mine === this.token) {
                this.phase = nextVoicePhase(this.phase, 'finish');
                this.modelProgress = 0;
            }
        }
    }

    /** Cancels an active recording without sending audio for transcription. */
    cancel(): void {
        if (this.phase !== 'recording') return;
        this.token++;
        this.phase = nextVoicePhase(this.phase, 'cancel');
        this.stopTimer();
        const recorder = this.recorder;
        if (recorder && recorder.state !== 'inactive') {
            recorder.onstop = () => this.releaseMedia();
            recorder.stop();
        } else this.releaseMedia();
    }

    private stopRecorder(): Promise<Blob> {
        const recorder = this.recorder;
        if (!recorder)
            return Promise.reject(new Error('The microphone is not recording'));
        return new Promise((resolve, reject) => {
            recorder.onerror = () => {
                this.releaseMedia();
                reject(new Error('The microphone stopped unexpectedly'));
            };
            recorder.onstop = () => {
                const blob = new Blob(this.chunks, { type: recorder.mimeType });
                this.releaseMedia();
                if (blob.size > 0) resolve(blob);
                else
                    reject(
                        new Error('The microphone did not record any audio'),
                    );
            };
            recorder.stop();
        });
    }

    private stopTimer(): void {
        if (this.timer) clearInterval(this.timer);
        this.timer = null;
    }

    private startMeter(stream: MediaStream): void {
        const context = new AudioContext();
        const source = context.createMediaStreamSource(stream);
        const analyser = context.createAnalyser();
        analyser.fftSize = 128;
        analyser.smoothingTimeConstant = 0.3;
        source.connect(analyser);
        if (context.state === 'suspended') void context.resume();
        const freqData = new Uint8Array(analyser.frequencyBinCount);
        this.meterContext = context;
        this.meterSource = source;
        this.level = 0;
        this.levels = [0, 0, 0, 0, 0];
        const sample = () => {
            analyser.getByteFrequencyData(freqData);
            let sum = 0;
            for (let i = 0; i < 20; i++) sum += freqData[i];
            const avg = sum / (20 * 255);
            this.level = this.level * 0.7 + avg * 0.3;

            const bands = [
                freqData[1],
                (freqData[2] + freqData[3]) * 0.5,
                (freqData[4] + freqData[5] + freqData[6]) / 3,
                (freqData[7] + freqData[8] + freqData[9]) / 3,
                (freqData[10] + freqData[11] + freqData[12] + freqData[13]) *
                    0.25,
            ];

            const current = [...this.levels];
            for (let i = 0; i < 5; i++) {
                const target = Math.min(1, Math.max(0, (bands[i] - 12) / 100));
                if (target > current[i]) {
                    current[i] = current[i] * 0.2 + target * 0.8;
                } else {
                    current[i] = current[i] * 0.76 + target * 0.24;
                }
            }
            this.levels = current;
            this.meterFrame = requestAnimationFrame(sample);
        };
        this.meterFrame = requestAnimationFrame(sample);
    }

    private stopMeter(): void {
        if (this.meterFrame) cancelAnimationFrame(this.meterFrame);
        this.meterFrame = 0;
        this.meterSource?.disconnect();
        this.meterSource = null;
        if (this.meterContext) void this.meterContext.close();
        this.meterContext = null;
        this.level = 0;
        this.levels = [0, 0, 0, 0, 0];
    }

    private releaseMedia(): void {
        this.stopTimer();
        this.stopMeter();
        this.stream?.getTracks().forEach((track) => track.stop());
        this.stream = null;
        this.recorder = null;
        this.chunks = [];
    }
}

const voices = new Map<string, VoiceCapture>();

/** Returns the independent voice capture state owned by one chat session. */
export function voiceOf(sessionId: string): VoiceCapture {
    let voice = voices.get(sessionId);
    if (!voice) {
        voice = new VoiceCapture();
        voices.set(sessionId, voice);
    }
    return voice;
}
