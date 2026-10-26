<script lang="ts">
    import { onDestroy, onMount } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { ZDrawer } from '@shival99/z-ui-svelte';
    import { capturePageIfPossible } from '../browser.svelte';
    import { guide } from '../guide.svelte';
    import { workspace } from '../sessions.svelte';
    import { openProjectFile } from '../file-viewer.svelte';
    import { toast } from '../toast';
    import { listen } from '../tauri-event';

    let { open = $bindable(false) }: { open: boolean } = $props();
    let recording = $state(false);
    let recordingSize = $state('');
    let recordingPath = $state('');
    let videoUrl = $state('');
    let videoData = $state('');
    let videoChat = $state('');
    let canvas: HTMLCanvasElement | undefined = $state();
    let recorder: MediaRecorder | undefined;
    let recordingChunks: BlobPart[] = [];
    let recordingStream: MediaStream | undefined;
    let recordingTarget: { root: string; fileName: string } | undefined;
    let recordingLoop = false;
    let frameCapture: Promise<void> | undefined;
    const currentTask = $derived(guide.tasks[workspace.activeId]);
    const currentProjectRoot = $derived(
        workspace.projects.find(
            (project) =>
                project.id ===
                workspace.sessions.find(
                    (session) => session.id === workspace.activeId,
                )?.projectId,
        )?.path ?? '',
    );
    const currentVideoUrl = $derived(
        videoChat === workspace.activeId ? videoUrl : '',
    );
    const isCurrentRecording = $derived(
        recording && videoChat === workspace.activeId,
    );
    const hasCurrentRecording = $derived(
        Boolean(recordingPath) && videoChat === workspace.activeId,
    );

    async function captureFrame() {
        if (frameCapture) {
            await frameCapture;
            return;
        }
        const task = captureFrameOnce();
        frameCapture = task;
        try {
            await task;
        } finally {
            if (frameCapture === task) frameCapture = undefined;
        }
    }

    async function captureFrameOnce() {
        if (!canvas) throw new Error('The guide recorder is not mounted.');
        const image = await capturePageIfPossible(1.5);
        if (!image) throw new Error('Could not capture the Preview.');
        const bitmap = await createImageBitmap(
            await (await fetch(image)).blob(),
        );
        if (canvas.width !== bitmap.width || canvas.height !== bitmap.height) {
            canvas.width = bitmap.width;
            canvas.height = bitmap.height;
        }
        recordingSize = `${bitmap.width} × ${bitmap.height}`;
        canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
        bitmap.close();
    }

    async function startRecording() {
        if (!canvas || !('MediaRecorder' in window) || !canvas.captureStream) {
            throw new Error(
                'Video recording is not supported in this WebView.',
            );
        }
        if (recording || recorder)
            throw new Error('A guide recording is already running.');
        videoData = '';
        videoUrl = '';
        recordingSize = '';
        recordingChunks = [];
        await captureFrame();
        recordingStream = canvas.captureStream(0);
        const mimeType = [
            'video/webm;codecs=vp9',
            'video/webm;codecs=vp8',
            'video/webm',
        ].find((type) => MediaRecorder.isTypeSupported(type));
        recorder = new MediaRecorder(recordingStream, {
            ...(mimeType ? { mimeType } : {}),
            videoBitsPerSecond: 3_000_000,
        });
        recorder.ondataavailable = (event) =>
            event.data.size && recordingChunks.push(event.data);
        recorder.start(1000);
        recording = true;
        recordingLoop = true;
        recordingStream.getVideoTracks().forEach((track) => {
            if ('requestFrame' in track)
                (track as CanvasCaptureMediaStreamTrack).requestFrame();
        });
        setTimeout(() => void captureRecordingFrame(), 250);
    }

    async function captureRecordingFrame() {
        if (!recordingLoop || !recorder || !recordingStream) return;
        try {
            await captureFrame();
            recordingStream.getVideoTracks().forEach((track) => {
                if ('requestFrame' in track)
                    (track as CanvasCaptureMediaStreamTrack).requestFrame();
            });
        } catch (error) {
            recordingLoop = false;
            toast.error(`Recording stopped: ${String(error)}`);
            await stopRecording().catch(() => undefined);
            return;
        }
        if (recordingLoop) setTimeout(() => void captureRecordingFrame(), 250);
    }

    async function stopRecording(): Promise<string> {
        recordingLoop = false;
        recording = false;
        const activeRecorder = recorder;
        if (!activeRecorder) return videoData;
        const finalized = new Promise<string>((resolve, reject) => {
            activeRecorder.onstop = () => {
                const blob = new Blob(recordingChunks, { type: 'video/webm' });
                videoUrl = URL.createObjectURL(blob);
                const reader = new FileReader();
                reader.onload = () => {
                    videoData = String(reader.result);
                    resolve(videoData);
                };
                reader.onerror = () =>
                    reject(
                        reader.error ??
                            new Error('Could not finalize the recording.'),
                    );
                reader.readAsDataURL(blob);
            };
        });
        await frameCapture?.catch(() => undefined);
        await captureFrame().catch(() => undefined);
        recordingStream?.getVideoTracks().forEach((track) => {
            if ('requestFrame' in track)
                (track as CanvasCaptureMediaStreamTrack).requestFrame();
        });
        activeRecorder.stop();
        recorder = undefined;
        recordingStream?.getTracks().forEach((track) => track.stop());
        recordingStream = undefined;
        return finalized;
    }

    type RecordingControl = {
        requestId: string;
        action: 'start' | 'stop';
        chat: string;
        root?: string;
        fileName?: string;
    };

    async function replyRecordingControl(
        requestId: string,
        ok: boolean,
        message: string,
    ) {
        await invoke('browser_guide_record_reply', { requestId, ok, message });
    }

    async function handleRecordingControl(control: RecordingControl) {
        if (control.action === 'start') {
            if (!control.root) {
                await replyRecordingControl(
                    control.requestId,
                    false,
                    'The project root is required to save the video.',
                );
                return;
            }
            recordingTarget = {
                root: control.root,
                fileName: control.fileName || `guide-${Date.now()}.webm`,
            };
            videoChat = control.chat;
            recordingPath = '';
            try {
                await startRecording();
                await replyRecordingControl(
                    control.requestId,
                    true,
                    `Recording is active at ${recordingSize}. Continue the requested browser workflow; frames are captured after browser actions.`,
                );
            } catch (error) {
                recordingTarget = undefined;
                await replyRecordingControl(
                    control.requestId,
                    false,
                    String(error),
                );
            }
            return;
        }
        const target = recordingTarget;
        if (!target) {
            await replyRecordingControl(
                control.requestId,
                false,
                'No agent-owned recording is running. Call browser_record_start first.',
            );
            return;
        }
        try {
            const dataUrl = await stopRecording();
            if (!dataUrl)
                throw new Error('The recording contains no video data.');
            const relativePath = `docs/user-guides/${target.fileName}`;
            const path = await invoke<string>('browser_save_recording', {
                root: target.root,
                relativePath,
                dataUrl,
            });
            recordingTarget = undefined;
            recordingPath = path;
            const task = guide.tasks[control.chat];
            if (task) {
                task.outputPath = path;
                task.progress = 'Video saved and ready to preview';
            }
            await replyRecordingControl(
                control.requestId,
                true,
                `Video saved to ${path} (${recordingSize}).`,
            );
        } catch (error) {
            await replyRecordingControl(
                control.requestId,
                false,
                String(error),
            );
        }
    }

    onMount(() => {
        let unlisten: (() => void) | undefined;
        let mounted = true;
        void listen<RecordingControl>(
            'agentdeck:guide-record-control',
            ({ payload }) => {
                void handleRecordingControl(payload);
            },
        ).then((stop) => {
            if (mounted) unlisten = stop;
            else stop();
        });
        return () => {
            mounted = false;
            unlisten?.();
        };
    });

    onDestroy(() => {
        if (recording) void stopRecording().catch(() => undefined);
        if (videoUrl) URL.revokeObjectURL(videoUrl);
    });
</script>

<ZDrawer
    bind:open
    zPlacement="right"
    zWidth="min(560px, 92vw)"
    zFloating
    zTitle="Document task"
    zClosable
>
    <div class="flex min-h-0 flex-col gap-3 text-xs">
        <p
            class="rounded-md border border-border bg-muted/30 p-3 text-muted-foreground"
        >
            Gửi yêu cầu bằng <code>/document</code> trong chat. Agent tự thao tác
            Preview, tạo tài liệu và lưu kết quả; bảng này chỉ theo dõi tiến trình
            và xem video.
        </p>
        <div
            class="flex items-center gap-2 rounded-md border border-border p-3"
        >
            <span
                class="size-2 rounded-full {isCurrentRecording
                    ? 'animate-pulse bg-primary'
                    : currentTask?.status === 'done' || hasCurrentRecording
                      ? 'bg-green-500'
                      : 'bg-muted-foreground/40'}"
            ></span>
            <span
                >{currentTask?.progress ??
                    (isCurrentRecording
                        ? `Đang ghi thao tác agent · ${recordingSize}`
                        : hasCurrentRecording
                          ? 'Đã hoàn tất video'
                          : 'Chưa có tác vụ tài liệu')}</span
            >
        </div>
        {#if currentTask}
            <p class="rounded-md bg-muted/30 p-3 text-muted-foreground">
                <span class="font-medium text-foreground">Yêu cầu:</span>
                {currentTask.request}
            </p>
            {#if currentTask.status !== 'working'}
                <div
                    class="max-h-48 overflow-auto whitespace-pre-wrap rounded-md border border-border p-3"
                >
                    {currentTask.result || 'Tác vụ đã hoàn tất.'}
                </div>
            {/if}
            {#if currentTask.outputPath}<button
                    type="button"
                    class="break-all rounded-md bg-muted/30 p-2 text-left text-primary hover:underline"
                    disabled={!currentProjectRoot}
                    onclick={() =>
                        openProjectFile(
                            currentTask.outputPath!,
                            currentProjectRoot,
                        )}>Mở kết quả: {currentTask.outputPath}</button
                >{/if}
        {/if}
        {#if currentVideoUrl}<video
                controls
                src={currentVideoUrl}
                class="max-h-[55vh] w-full rounded bg-black"
                ><track
                    kind="captions"
                    src="data:text/vtt,WEBVTT"
                    srclang="en"
                    label="Captions"
                /></video
            >{:else}<div
                class="grid min-h-48 place-items-center rounded-md border border-dashed border-border text-muted-foreground"
            >
                Tiến trình chi tiết và đường dẫn tài liệu nằm trong chat hiện
                tại.
            </div>{/if}
        {#if hasCurrentRecording}<p
                class="break-all rounded-md bg-muted/30 p-2 text-muted-foreground"
            >
                Video: {recordingPath}
            </p>{/if}
    </div>
</ZDrawer>
<canvas bind:this={canvas} class="hidden"></canvas>
