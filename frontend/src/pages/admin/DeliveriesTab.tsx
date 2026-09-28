import { useState } from 'react';
import Select from '@/elements/input/Select.tsx';
import Group from '@/elements/layout/Group.tsx';
import { useSearchableResource } from '@/plugins/resource/useSearchableResource.ts';
import { useAdminCan } from '@/plugins/usePermissions.ts';
import {
  adminWebhookApi,
  type DeliveryStatus,
  deliveryStatusSchema,
  getAdminWebhooks,
  type Webhook,
} from '../../api.ts';
import DeliveriesTable from '../../components/DeliveriesTable.tsx';
import { useExtTranslations } from '../../translations.ts';
import useAdminEvents from './useAdminEvents.ts';

export type WebhookRef = { uuid: string; name: string };

export default function DeliveriesTab({
  webhook,
  onWebhookChange,
}: {
  webhook: WebhookRef | null;
  onWebhookChange: (webhook: WebhookRef | null) => void;
}) {
  const { t: tExt } = useExtTranslations();
  const canManage = useAdminCan('webhooks.manage');
  const { data: catalog } = useAdminEvents();

  const [status, setStatus] = useState<DeliveryStatus | null>(null);
  const [event, setEvent] = useState<string | null>(null);

  const webhooks = useSearchableResource<Webhook>({
    queryKey: [...adminWebhookApi.queryKey, 'webhooks', 'select'],
    fetcher: (search) => getAdminWebhooks(1, search),
  });

  const webhookOptions = webhooks.items.map((item) => ({ value: item.uuid, label: item.name }));
  if (webhook && !webhookOptions.some((option) => option.value === webhook.uuid)) {
    webhookOptions.unshift({ value: webhook.uuid, label: webhook.name });
  }

  return (
    <>
      <Group justify='flex-end' mb='md'>
        <Select
          placeholder={tExt('deliveries.allWebhooks', {})}
          data={webhookOptions}
          value={webhook?.uuid ?? null}
          onChange={(uuid) => {
            const option = webhookOptions.find((item) => item.value === uuid);
            onWebhookChange(option ? { uuid: option.value, name: option.label } : null);
          }}
          searchable
          searchValue={webhooks.search}
          onSearchChange={webhooks.setSearch}
          loading={webhooks.loading}
          clearable
          w={240}
        />
        <Select
          placeholder={tExt('deliveries.allEvents', {})}
          data={(catalog?.events ?? []).map((item) => ({ value: item.id, label: item.label }))}
          value={event}
          onChange={setEvent}
          searchable
          clearable
          w={240}
        />
        <Select
          placeholder={tExt('deliveries.allStatuses', {})}
          data={deliveryStatusSchema.options.map((value) => ({ value, label: tExt(`status.${value}`, {}) }))}
          value={status}
          onChange={(value) => setStatus(value as DeliveryStatus | null)}
          clearable
          w={180}
        />
      </Group>

      <DeliveriesTable
        api={adminWebhookApi}
        filters={{ webhook: webhook?.uuid, status, event }}
        events={catalog?.events ?? []}
        showWebhook
        canRedeliver={canManage}
      />
    </>
  );
}
