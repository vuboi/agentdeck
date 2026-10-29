/** Shared document-task state; the drawer stays mounted while the agent and Preview work in the background. */
export type DocumentTask = {
    request: string;
    status: 'working' | 'done' | 'error';
    progress: string;
    result: string;
    outputPath?: string;
};

export const guide = $state({
    open: false,
    tasks: {} as Record<string, DocumentTask>,
});
