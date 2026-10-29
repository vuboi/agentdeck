<script lang="ts">
  import { ZButton, ZIcon } from '@shival99/z-ui-svelte';
  import { showSession } from '../../app/ui/content-tabs.svelte';
  import { answerPermission, answerQuestions, cancelPending, resumes, workspace, type Agent } from '../../chat/sessions.svelte';
  import ResumeCard from './ResumeCard.svelte';
  import PermissionCard from './PermissionCard.svelte';
  import QuestionCard from './QuestionCard.svelte';
  import type { Block } from '../../chat/mock';
  import type { Question } from '../../chat/recovery/questions';
  import Composer from './Composer.svelte';
  import { agentName } from './chat-helpers';

  type Approval = { block: Extract<Block, { kind: 'permission' }>; messageId: string; index: number; agent: Agent | undefined };
  type Pending = { messageId: string; questions: Question[]; agent: Agent | undefined };
  let { pendingApproval, pendingQuestion, stepApprovals }: {
    pendingApproval: Approval | null;
    pendingQuestion: Pending | null;
    stepApprovals: { childId: string; step: string; agent: string }[];
  } = $props();
</script>

<!-- A pending approval sits right above the input, where the eye already is. -->
<!-- The chat stopped at a usage limit: it carries on by itself when the limit resets. -->
{#if resumes[workspace.activeId]}<ResumeCard resume={resumes[workspace.activeId]} />{/if}
{#if pendingApproval}
  <div class="mb-2">
    <PermissionCard block={pendingApproval.block} agent={pendingApproval.agent} onanswer={(answer) => answerPermission(pendingApproval.messageId, pendingApproval.index, answer)} />
  </div>
{/if}
{#if pendingQuestion}
  <div class="mb-2">
    <QuestionCard questions={pendingQuestion.questions} from={pendingQuestion.agent ? agentName(pendingQuestion.agent) : ''} onsubmit={(text) => answerQuestions(workspace.activeId, text)} oncancel={() => cancelPending(workspace.activeId)} />
  </div>
{/if}
<!-- A flow step that needs approval waits in its own chat: say which, and take the user there. -->
{#each stepApprovals as item (item.childId)}
  <div class="mb-2 flex items-center gap-2 rounded-xl border border-warning/50 bg-warning/5 px-3 py-2 text-xs" role="status">
    <ZIcon zType="lucideShieldAlert" class="size-3.5 shrink-0 text-warning" />
    <span class="min-w-0 flex-1 truncate"><span class="font-medium">{item.step}</span> ({item.agent}) needs your approval to continue</span>
    <ZButton zWave={false} zType="default" zSize="sm" class="h-7 shrink-0 px-3 text-xs" onclick={() => showSession(item.childId)}>Open step</ZButton>
  </div>
{/each}
<Composer />
