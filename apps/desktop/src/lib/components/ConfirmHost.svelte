<script lang="ts">
  import { ZModal } from '@shival99/z-ui-svelte';
  import { answerConfirm, confirmBox } from '../confirm.svelte';

  // Draws the box `askConfirm` asks for. Closing it any way but OK answers "no".
  let open = $state(false);
  $effect(() => {
    open = confirmBox.open;
  });
  $effect(() => {
    if (!open && confirmBox.open) answerConfirm(false);
  });
</script>

<ZModal
  bind:open
  zTitle={confirmBox.request?.title ?? ''}
  zWidth="420px"
  zBodyClass="py-3"
  zCancelText="Cancel"
  zOkText={confirmBox.request?.ok ?? 'OK'}
  onOk={() => answerConfirm(true)}
>
  <p class="m-0 text-xs leading-relaxed whitespace-pre-line {confirmBox.request?.danger ? 'text-destructive' : 'text-foreground'}">{confirmBox.request?.text ?? ''}</p>
</ZModal>
