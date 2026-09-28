import { Center, SimpleGrid } from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import { AdminCan } from '@/elements/Can.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import NumberInput from '@/elements/input/NumberInput.tsx';
import Switch from '@/elements/input/Switch.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import { useAdminCan } from '@/plugins/usePermissions.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { getAdminSettings, updateAdminSettings, type WebhookSettings } from '../../api.ts';
import { useExtTranslations } from '../../translations.ts';

const settingsQueryKey = ['dev.caloptreyx.webhooks', 'admin', 'settings'] as const;

const clamp = (value: string | number, min: number, max: number, fallback: number) => {
  const parsed = typeof value === 'number' ? value : Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? Math.min(max, Math.max(min, Math.trunc(parsed))) : fallback;
};

export default function SettingsTab() {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const canManage = useAdminCan('webhooks.manage');

  const { data, isLoading, error } = useQuery({
    queryKey: settingsQueryKey,
    queryFn: getAdminSettings,
  });

  const [settings, setSettings] = useState<WebhookSettings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (data) {
      setSettings(data);
    }
  }, [data]);

  if (isLoading) {
    return (
      <Center py='lg'>
        <Spinner />
      </Center>
    );
  }

  if (error || !settings) {
    return <Alert color='red'>{error ? httpErrorToHuman(error) : null}</Alert>;
  }

  const update = (patch: Partial<WebhookSettings>) => setSettings({ ...settings, ...patch });

  const pollIntervalValid =
    settings.statePollIntervalSeconds === 0 ||
    (settings.statePollIntervalSeconds >= 5 && settings.statePollIntervalSeconds <= 3600);

  const doSave = async () => {
    if (!pollIntervalValid) return;
    setSaving(true);
    try {
      await updateAdminSettings(settings);
      addToast(tExt('pages.admin.settings.saved', {}), 'success');
      queryClient.invalidateQueries({ queryKey: settingsQueryKey });
    } catch (msg) {
      addToast(httpErrorToHuman(msg), 'error');
    }
    setSaving(false);
  };

  return (
    <Stack gap='lg'>
      <TitleCard title={tExt('pages.admin.settings.deliveryTitle', {})}>
        <SimpleGrid cols={{ base: 1, md: 2 }}>
          <NumberInput
            label={tExt('pages.admin.settings.maxAttempts', {})}
            description={tExt('pages.admin.settings.maxAttemptsDescription', {})}
            value={settings.maxAttempts}
            onChange={(value) => update({ maxAttempts: clamp(value, 1, 20, 5) })}
            min={1}
            max={20}
            disabled={!canManage}
          />
          <NumberInput
            label={tExt('pages.admin.settings.requestTimeoutSeconds', {})}
            description={tExt('pages.admin.settings.requestTimeoutSecondsDescription', {})}
            value={settings.requestTimeoutSeconds}
            onChange={(value) => update({ requestTimeoutSeconds: clamp(value, 1, 60, 10) })}
            min={1}
            max={60}
            disabled={!canManage}
          />
          <NumberInput
            label={tExt('pages.admin.settings.retentionDays', {})}
            description={tExt('pages.admin.settings.retentionDaysDescription', {})}
            value={settings.retentionDays}
            onChange={(value) => update({ retentionDays: clamp(value, 1, 365, 14) })}
            min={1}
            max={365}
            disabled={!canManage}
          />
          <NumberInput
            label={tExt('pages.admin.settings.statePollIntervalSeconds', {})}
            description={tExt('pages.admin.settings.statePollIntervalSecondsDescription', {})}
            value={settings.statePollIntervalSeconds}
            onChange={(value) => update({ statePollIntervalSeconds: clamp(value, 0, 3600, 0) })}
            error={pollIntervalValid ? undefined : tExt('pages.admin.settings.invalidPollInterval', {})}
            min={0}
            max={3600}
            disabled={!canManage}
          />
        </SimpleGrid>
      </TitleCard>

      <TitleCard title={tExt('pages.admin.settings.userWebhooksTitle', {})}>
        <Stack gap='md'>
          <SimpleGrid cols={{ base: 1, md: 2 }}>
            <Stack gap='md'>
              <Switch
                label={tExt('pages.admin.settings.serverWebhooksEnabled', {})}
                description={tExt('pages.admin.settings.serverWebhooksEnabledDescription', {})}
                checked={settings.serverWebhooksEnabled}
                onChange={(e) => update({ serverWebhooksEnabled: e.target.checked })}
                disabled={!canManage}
              />
              <NumberInput
                label={tExt('pages.admin.settings.serverWebhookLimit', {})}
                description={tExt('pages.admin.settings.serverWebhookLimitDescription', {})}
                value={settings.serverWebhookLimit}
                onChange={(value) => update({ serverWebhookLimit: clamp(value, 0, 100, 0) })}
                min={0}
                max={100}
                disabled={!canManage || !settings.serverWebhooksEnabled}
              />
            </Stack>
            <Stack gap='md'>
              <Switch
                label={tExt('pages.admin.settings.accountWebhooksEnabled', {})}
                description={tExt('pages.admin.settings.accountWebhooksEnabledDescription', {})}
                checked={settings.accountWebhooksEnabled}
                onChange={(e) => update({ accountWebhooksEnabled: e.target.checked })}
                disabled={!canManage}
              />
              <NumberInput
                label={tExt('pages.admin.settings.accountWebhookLimit', {})}
                description={tExt('pages.admin.settings.accountWebhookLimitDescription', {})}
                value={settings.accountWebhookLimit}
                onChange={(value) => update({ accountWebhookLimit: clamp(value, 0, 100, 0) })}
                min={0}
                max={100}
                disabled={!canManage || !settings.accountWebhooksEnabled}
              />
            </Stack>
          </SimpleGrid>
          <Switch
            label={tExt('pages.admin.settings.allowPrivateTargets', {})}
            description={tExt('pages.admin.settings.allowPrivateTargetsDescription', {})}
            checked={settings.allowPrivateTargets}
            onChange={(e) => update({ allowPrivateTargets: e.target.checked })}
            disabled={!canManage || !(settings.serverWebhooksEnabled || settings.accountWebhooksEnabled)}
          />
        </Stack>
      </TitleCard>

      <Group justify='flex-end'>
        <AdminCan action='webhooks.manage' cantSave>
          <Button onClick={doSave} loading={saving} disabled={!pollIntervalValid}>
            {tExt('pages.admin.settings.save', {})}
          </Button>
        </AdminCan>
      </Group>
    </Stack>
  );
}
