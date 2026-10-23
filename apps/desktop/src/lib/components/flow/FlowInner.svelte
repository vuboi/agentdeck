<script lang="ts">
  import '@xyflow/svelte/dist/style.css';
  import './flow.css';
  import { onMount, untrack } from 'svelte';
  import { Background, BackgroundVariant, MiniMap, Panel, SvelteFlow, useSvelteFlow, type Connection, type Edge, type Node, type NodeTypes, type EdgeTypes } from '@xyflow/svelte';
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import type { ZIconName } from '@shival99/z-ui-svelte';
  import type { DelegateRun } from '../../sessions.svelte';
  import { addLink, inputsOf, newStep, normalizeGraph, removeLink, removeStep, wouldCycle, type WorkflowStep } from '../../workflows.svelte';
  import EndNode from './EndNode.svelte';
  import FlowEdge from './FlowEdge.svelte';
  import StepNode from './StepNode.svelte';
  import { buildFlow } from './flow-graph';
  import { confirmDelete } from './confirm';

  let {
    steps = $bindable(),
    selected = $bindable(),
    mode,
    run,
    minimap = false,
  }: { steps: WorkflowStep[]; selected?: string | null; mode: 'edit' | 'run'; run?: DelegateRun; minimap?: boolean } = $props();

  const icon = (name: string) => name as ZIconName;
  const nodeTypes: NodeTypes = { step: StepNode, end: EndNode } as unknown as NodeTypes;
  const edgeTypes: EdgeTypes = { flow: FlowEdge } as unknown as EdgeTypes;
  const flow = useSvelteFlow();

  const graph = $derived(normalizeGraph(steps));
  let nodes = $state.raw<Node[]>([]);
  let edges = $state.raw<Edge[]>([]);
  let box = $state<HTMLElement | null>(null);
  let zoom = $state(1);
  /** The cards and lines ease in only while the canvas opens, never again when the flow is rebuilt after an edit. */
  let entering = $state(true);
  onMount(() => {
    const timer = setTimeout(() => (entering = false), 1600);
    return () => clearTimeout(timer);
  });
  /** An end of a link is being moved: letting go on empty space must not add a card. */
  let moving = false;

  /** Minimap colours: the two ends in the accent, a working step brighter, the rest muted. */
  const miniColor = (node: Node) =>
    node.type === 'end' ? 'color-mix(in srgb, var(--primary) 55%, transparent)' : (node.data as { state?: string }).state === 'running' ? 'var(--primary)' : 'color-mix(in srgb, var(--muted-foreground) 45%, transparent)';

  const cut = (from: string, to: string) => (steps = removeLink(normalizeGraph(steps), from, to));

  // The canvas follows the flow (and, in a run, the state of every step); a card being dragged is left alone.
  $effect(() => {
    const built = buildFlow(graph, mode, run, cut);
    untrack(() => {
      nodes = built.nodes.map((node) => ({ ...node, selected: node.id === selected }));
      edges = built.edges;
    });
  });

  // The app's theme, for the canvas's own colours.
  let dark = $state(false);
  onMount(() => {
    const read = () => (dark = document.documentElement.classList.contains('dark') || document.documentElement.dataset.theme === 'dark');
    read();
    const watch = new MutationObserver(read);
    watch.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] });
    return () => watch.disconnect();
  });

  /** Every card keeps the place it has on the canvas now. */
  const pinned = () =>
    graph.map((step) => {
      const node = nodes.find((item) => item.id === step.id);
      return node ? { ...step, x: Math.round(node.position.x), y: Math.round(node.position.y) } : step;
    });

  /** Adds a step fed by `from`, then lays every card out again in rows so none sit on top of each other. */
  function addAt(from: string) {
    const item: WorkflowStep = { ...newStep(graph.length + 1), inputs: [from] };
    steps = normalizeGraph([...graph, item]).map((step) => ({ ...step, x: undefined, y: undefined }));
    selected = item.id;
    setTimeout(() => void flow.fitView({ padding: 0.2, maxZoom: 1, duration: 300 }), 60);
  }
  function addCentre() {
    const last = graph.filter((step) => !graph.some((other) => inputsOf(graph)[other.id]?.includes(step.id))).at(-1);
    addAt(last?.id ?? 'start');
  }
  /** Zooms to `level` (kept within the canvas limits), keeping the middle of the view where it is. */
  function zoomAt(level: number) {
    const view = flow.getViewport();
    const next = Math.min(1.75, Math.max(0.25, level));
    const cx = (box?.clientWidth ?? 0) / 2;
    const cy = (box?.clientHeight ?? 0) / 2;
    void flow.setViewport({ zoom: next, x: cx - ((cx - view.x) / view.zoom) * next, y: cy - ((cy - view.y) / view.zoom) * next }, { duration: 200 });
  }
  const resetZoom = () => zoomAt(1);

  // Right click on a step: a small menu to delete it (or open its settings) at once.
  let menu = $state<{ id: string; x: number; y: number } | null>(null);
  function openMenu(id: string, event: MouseEvent) {
    event.preventDefault();
    if (mode !== 'edit' || id === 'start' || id === 'end') return;
    const rect = box?.getBoundingClientRect();
    menu = { id, x: event.clientX - (rect?.left ?? 0), y: event.clientY - (rect?.top ?? 0) };
  }
  async function deleteStep(id: string) {
    menu = null;
    if (!(await confirmDelete([graph.find((step) => step.id === id)?.name || 'Untitled step']))) return;
    steps = removeStep(normalizeGraph(steps), id);
    if (selected === id) selected = null;
  }

  function tidy() {
    steps = graph.map((step) => ({ ...step, x: undefined, y: undefined }));
    setTimeout(() => void flow.fitView({ padding: 0.2, duration: 300 }), 30);
  }

  /** The card under the pointer when a line is let go, if any. */
  function cardAt(event: MouseEvent | TouchEvent): string | null {
    const point = 'changedTouches' in event ? event.changedTouches[0] : event;
    const element = document.elementFromPoint(point.clientX, point.clientY)?.closest<HTMLElement>('.svelte-flow__node');
    return element?.dataset.id ?? null;
  }

  const valid = (connection: Edge | Connection | { source: string; target: string }) =>
    mode === 'edit' &&
    connection.source !== 'end' &&
    connection.target !== 'start' &&
    connection.target !== 'end' &&
    !wouldCycle(graph, connection.source, connection.target) &&
    !inputsOf(graph)[connection.target]?.includes(connection.source);
</script>

<div bind:this={box} class="flow-canvas relative h-full w-full" class:flow-run={mode === 'run'} class:entering>
  <SvelteFlow
    bind:nodes
    bind:edges
    {nodeTypes}
    {edgeTypes}
    colorMode={dark ? 'dark' : 'light'}
    fitView
    fitViewOptions={{ padding: 0.2, maxZoom: 1 }}
    minZoom={0.25}
    maxZoom={1.75}
    nodesConnectable={mode === 'edit'}
    deleteKey={mode === 'edit' ? ['Delete', 'Backspace'] : null}
    connectionRadius={36}
    proOptions={{ hideAttribution: true }}
    isValidConnection={valid}
    oninit={() => (zoom = flow.getZoom())}
    onmove={(_, viewport) => (zoom = viewport.zoom)}
    onconnect={(connection) => (steps = addLink(graph, connection.source, connection.target))}
    onreconnectstart={() => (moving = true)}
    onconnectend={(event, state) => {
      if (moving || mode !== 'edit' || state.isValid || !state.fromNode) return;
      // Let go over a card (not only its dot): link to that card. Over empty space: a new card there.
      const over = cardAt(event);
      const from = state.fromNode.id;
      const down = state.fromHandle?.type !== 'target';
      if (over) {
        const link = down ? { source: from, target: over } : { source: over, target: from };
        if (valid(link)) steps = addLink(graph, link.source, link.target);
        return;
      }
      if (down && !state.toNode) addAt(from);
    }}
    onreconnect={(old, next) => {
      if (!next.source || !next.target) return;
      steps = addLink(removeLink(graph, old.source, old.target), next.source, next.target);
    }}
    onreconnectend={(event, old, _, state) => {
      setTimeout(() => (moving = false), 50);
      if (state.isValid) return;
      const over = cardAt(event);
      const moved = removeLink(graph, old.source, old.target);
      // Dropped on a card body: move the link's loose end there; on empty space: the link comes off.
      if (over && old.source !== over && valid({ source: old.source, target: over })) steps = addLink(moved, old.source, over);
      else steps = moved;
    }}
    onnodeclick={({ node }) => (selected = node.id)}
    onnodecontextmenu={({ node, event }) => openMenu(node.id, event)}
    onpaneclick={() => {
      selected = null;
      menu = null;
    }}
    onmovestart={() => (menu = null)}
    onnodedragstop={() => {
      if (mode === 'edit') steps = pinned();
    }}
    onbeforedelete={async ({ nodes: going }) => {
      const names = going.filter((node) => node.type === 'step').map((node) => graph.find((step) => step.id === node.id)?.name || 'Untitled step');
      return names.length === 0 || confirmDelete(names);
    }}
    ondelete={({ nodes: gone, edges: cutEdges }) => {
      let next = graph;
      for (const node of gone) if (node.id !== 'start' && node.id !== 'end') next = removeStep(next, node.id);
      const ids = new Set(next.map((step) => step.id));
      for (const edge of cutEdges) if (ids.has(edge.target) && (edge.source === 'start' || ids.has(edge.source))) next = removeLink(next, edge.source, edge.target);
      steps = next;
      if (gone.some((node) => node.id === selected)) selected = null;
    }}
  >
    <Background variant={BackgroundVariant.Dots} gap={18} size={1.2} />
    <Panel position="bottom-left">
      <div class="flex flex-col items-center gap-0.5 rounded-xl border border-border bg-card/95 p-1 shadow-lg backdrop-blur" role="toolbar" aria-label="Zoom">
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! p-0!" aria-label="Zoom in" onclick={() => zoomAt(zoom * 1.2)}><ZIcon zType={icon('lucidePlus')} class="size-4" /></ZButton>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="h-7! w-10! p-0! text-[11px] tabular-nums" aria-label="Reset zoom to 100%" onclick={resetZoom}>{Math.round(zoom * 100)}%</ZButton>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! p-0!" aria-label="Zoom out" onclick={() => zoomAt(zoom / 1.2)}><ZIcon zType={icon('lucideMinus')} class="size-4" /></ZButton>
        <span class="my-0.5 h-px w-6 bg-border" aria-hidden="true"></span>
        <ZButton zWave={false} zType="ghost" zSize="sm" class="size-8! p-0!" aria-label="Fit the flow in view" onclick={() => void flow.fitView({ padding: 0.2, duration: 300 })}><ZIcon zType={icon('lucideMaximize2')} class="size-4" /></ZButton>
      </div>
    </Panel>
    {#if minimap}
      <MiniMap position="bottom-right" width={168} height={108} pannable zoomable nodeBorderRadius={18} nodeColor={miniColor} maskColor="color-mix(in srgb, var(--background) 62%, transparent)" maskStrokeColor="var(--primary)" maskStrokeWidth={1.5} class="flow-minimap" />
    {/if}
    {#if mode === 'edit'}
      <Panel position="bottom-center">
        <div class="flex items-center gap-1 rounded-xl border border-border bg-card/95 p-1 shadow-lg backdrop-blur" role="toolbar" aria-label="Flow tools">
          <ZButton zWave={false} zType="ghost" onclick={addCentre}><ZIcon zType={icon('lucidePlus')} class="size-4" />Add step</ZButton>
          <span class="h-5 w-px bg-border" aria-hidden="true"></span>
          <ZButton zWave={false} zType="ghost" onclick={tidy}><ZIcon zType={icon('lucideListChecks')} class="size-4" />Tidy</ZButton>
        </div>
      </Panel>
    {/if}
  </SvelteFlow>
  {#if menu}
    {@const target = menu}
    <div class="fixed inset-0 z-40" role="presentation" onpointerdown={() => (menu = null)} oncontextmenu={(event) => { event.preventDefault(); menu = null; }}></div>
    <div class="absolute z-50 min-w-44 rounded-xl border border-border bg-card p-1 shadow-xl" style="left:{target.x}px; top:{target.y}px" role="menu" aria-label="Step menu">
      <button type="button" role="menuitem" class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs hover:bg-accent" onclick={() => { selected = target.id; menu = null; }}>
        <ZIcon zType={icon('lucideSettings2')} class="size-3.5 text-muted-foreground" />Edit step
      </button>
      <button type="button" role="menuitem" class="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-destructive hover:bg-destructive/12" onclick={() => deleteStep(target.id)}>
        <ZIcon zType={icon('lucideTrash2')} class="size-3.5" />Delete step
      </button>
    </div>
  {/if}
</div>
