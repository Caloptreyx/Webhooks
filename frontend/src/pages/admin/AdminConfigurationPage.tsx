import { useState } from 'react';
import Tabs from '@/elements/layout/Tabs.tsx';
import { useExtTranslations } from '../../translations.ts';
import DeliveriesTab, { type WebhookRef } from './DeliveriesTab.tsx';
import SettingsTab from './SettingsTab.tsx';
import WebhooksTab from './WebhooksTab.tsx';

export default function AdminConfigurationPage() {
  const { t: tExt } = useExtTranslations();

  const [tab, setTab] = useState<string | null>('webhooks');
  const [deliveriesWebhook, setDeliveriesWebhook] = useState<WebhookRef | null>(null);

  return (
    <Tabs value={tab} onChange={setTab} keepMounted={false}>
      <Tabs.List>
        <Tabs.Tab value='webhooks'>{tExt('pages.admin.tabs.webhooks', {})}</Tabs.Tab>
        <Tabs.Tab value='deliveries'>{tExt('pages.admin.tabs.deliveries', {})}</Tabs.Tab>
        <Tabs.Tab value='settings'>{tExt('pages.admin.tabs.settings', {})}</Tabs.Tab>
      </Tabs.List>

      <Tabs.Panel value='webhooks' pt='md'>
        <WebhooksTab
          onViewDeliveries={(webhook) => {
            setDeliveriesWebhook({ uuid: webhook.uuid, name: webhook.name });
            setTab('deliveries');
          }}
        />
      </Tabs.Panel>
      <Tabs.Panel value='deliveries' pt='md'>
        <DeliveriesTab webhook={deliveriesWebhook} onWebhookChange={setDeliveriesWebhook} />
      </Tabs.Panel>
      <Tabs.Panel value='settings' pt='md'>
        <SettingsTab />
      </Tabs.Panel>
    </Tabs>
  );
}
