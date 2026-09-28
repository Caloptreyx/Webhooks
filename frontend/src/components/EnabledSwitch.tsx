import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Switch from '@/elements/input/Switch.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import type { Webhook, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

export default function EnabledSwitch({
  api,
  webhook,
  disabled,
  onChanged,
}: {
  api: WebhookScopeApi;
  webhook: Webhook;
  disabled: boolean;
  onChanged: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();

  const [saving, setSaving] = useState(false);

  const toggle = async (enabled: boolean) => {
    setSaving(true);
    try {
      await api.updateWebhook(webhook.uuid, { enabled });
      addToast(
        tExt('common.toggled', {
          name: webhook.name,
          state: enabled ? tExt('common.toggledEnabled', {}) : tExt('common.toggledDisabled', {}),
        }),
        'success',
      );
      onChanged();
    } catch (msg) {
      addToast(httpErrorToHuman(msg), 'error');
    }
    setSaving(false);
  };

  return (
    <Switch
      checked={webhook.enabled}
      onChange={(e) => toggle(e.target.checked)}
      disabled={disabled || saving}
      aria-label={tExt('table.enabled', {})}
    />
  );
}
