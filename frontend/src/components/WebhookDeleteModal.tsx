import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import type { Webhook, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

export default function WebhookDeleteModal({
  api,
  webhook,
  onClose,
  onDeleted,
}: {
  api: WebhookScopeApi;
  webhook: Webhook | null;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();

  const doDelete = async () => {
    if (!webhook) return;
    await api.deleteWebhook(webhook.uuid);
    addToast(tExt('common.deleted', {}), 'success');
    onClose();
    onDeleted();
  };

  return (
    <ConfirmationModal
      opened={!!webhook}
      onClose={onClose}
      title={tExt('common.deleteTitle', {})}
      confirm={tExt('common.delete', {})}
      onConfirmed={doDelete}
    >
      {tExt('common.deleteContent', { name: webhook?.name ?? '' }).md()}
    </ConfirmationModal>
  );
}
