import { zLoadIcon } from '@shival99/z-ui-svelte';

// Icons load lazily (memory, then IndexedDB, then CDN). Opening a popover for the first time after
// a reload used to wait for them and hitch, so warm the ones the app uses right after start-up.
const USED = [
  'brandClaude', 'brandOpenai', 'brandGemini',
  'lucideActivity', 'lucideAppWindow', 'lucideArrowUp', 'lucideAtSign', 'lucideBot', 'lucideBrain', 'lucideCheck',
  'lucideChevronDown', 'lucideChevronRight', 'lucideCircleCheck', 'lucideCircleX', 'lucideClipboardList',
  'lucideCopy', 'lucideEllipsis', 'lucideFilePen', 'lucideFolder', 'lucideGitCompare', 'lucideGitFork',
  'lucideGlobe', 'lucideHand', 'lucideLoaderCircle', 'lucideMaximize2', 'lucideMenu', 'lucideMessageSquare',
  'lucideMinimize2', 'lucideMinus', 'lucideMonitor', 'lucideMoon', 'lucidePalette', 'lucidePanelRightClose',
  'lucidePanelRightOpen', 'lucidePaperclip', 'lucidePlus', 'lucideShieldCheck', 'lucideSparkles', 'lucideSquare',
  'lucideSquareTerminal', 'lucideSun', 'lucideTerminal', 'lucideWorkflow', 'lucideX', 'lucideZap', 'lucideArrowLeft', 'lucideCoins', 'lucideArrowUpFromLine', 'lucideDatabaseZap', 'lucideDatabase', 'lucideArrowDownToLine', 'lucideListChecks', 'lucideGitBranchPlus', 'lucideEllipsisVertical', 'lucideSettings2', 'lucideClock', 'lucideMousePointer2', 'lucidePencil', 'lucideType', 'lucideExternalLink', 'lucideEllipsis', 'lucideBan', 'lucideHighlighter', 'lucideSquareTerminal', 'lucideNetwork', 'lucideArrowRight', 'lucideRotateCw', 'lucideSmartphone', 'lucideTablet', 'lucideMousePointerClick', 'lucidePenLine', 'lucideMoveUpRight', 'lucideUndo2', 'lucideTrash2', 'lucideMessageSquarePlus', 'lucideShieldAlert', 'lucideShieldX', 'lucideGripVertical',
];

/** Loads every icon the app uses, a few at a time, without blocking first paint. */
export function warmIcons() {
  const queue = [...USED];
  const worker = async () => {
    for (let name = queue.shift(); name; name = queue.shift()) await zLoadIcon(name);
  };
  const start = () => void Promise.all([worker(), worker(), worker(), worker()]);
  if ('requestIdleCallback' in window) window.requestIdleCallback(start, { timeout: 1500 });
  else setTimeout(start, 200);
}
