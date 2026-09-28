import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import type { Delivery, EventInfo, Webhook, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import DeliveryModal from './DeliveryModal.tsx';

// sends a test event and shows the resulting delivery; render `modal` once in the page
export default function useWebhookTest({
  api,
  events,
  canRedeliver,
  onTested,
}: {
  api: WebhookScopeApi;
  events: EventInfo[];
  canRedeliver: boolean;
  onTested: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();

  const [delivery, setDelivery] = useState<Delivery | null>(null);

  const run = async (webhook: Webhook) => {
    addToast(tExt('common.testSending', { name: webhook.name }), 'info');
    try {
      const result = await api.testWebhook(webhook.uuid);
      setDelivery(result);
      if (result.status === 'success') {
        addToast(tExt('common.testSent', { status: result.responseStatus ?? '-' }), 'success');
      } else {
        addToast(tExt('common.testFailed', {}), 'error');
      }
      onTested();
    } catch (msg) {
      addToast(httpErrorToHuman(msg), 'error');
    }
  };

  return {
    run,
    modal: (
      <DeliveryModal
        api={api}
        delivery={delivery}
        events={events}
        title={tExt('deliveries.testTitle', {})}
        canRedeliver={canRedeliver}
        onClose={() => setDelivery(null)}
      />
    ),
  };
}
