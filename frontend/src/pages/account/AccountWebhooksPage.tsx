import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import AccountContentContainer from '@/elements/containers/AccountContentContainer.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Text from '@/elements/typography/Text.tsx';
import { accountWebhookApi } from '../../api.ts';
import ClientWebhookList from '../../components/ClientWebhookList.tsx';
import CreateWebhookButton from '../../components/CreateWebhookButton.tsx';
import { useExtTranslations } from '../../translations.ts';

export default function AccountWebhooksPage() {
  const { t: tExt } = useExtTranslations();

  const [creating, setCreating] = useState(false);

  const query = useQuery({
    queryKey: [...accountWebhookApi.queryKey, 'webhooks'],
    queryFn: () => accountWebhookApi.listWebhooks(),
  });
  const { data } = query;

  return (
    <AccountContentContainer
      title={tExt('common.webhooks', {})}
      subtitle={tExt('common.usage', { count: data?.webhooks.length ?? 0, limit: data?.limit ?? 0 })}
      contentRight={
        data?.enabled && (
          <CreateWebhookButton
            webhooks={data}
            limitReachedText={tExt('pages.account.limitReached', { limit: data.limit })}
            onClick={() => setCreating(true)}
          />
        )
      }
    >
      <Stack gap='md'>
        <Text size='sm' c='dimmed'>
          {tExt('pages.account.description', {})}
        </Text>
        {/* every user holds all webhook permissions for their own account */}
        <ClientWebhookList
          api={accountWebhookApi}
          query={query}
          creating={creating}
          onCreateClose={() => setCreating(false)}
          canUpdate
          canDelete
          serverFilterPicker={{
            description: tExt('form.serverFilterAccountDescription', {}),
            emptyLabel: tExt('form.allMyServers', {}),
          }}
          disabledText={tExt('pages.account.disabled', {})}
          emptyText={tExt('pages.account.empty', {})}
        />
      </Stack>
    </AccountContentContainer>
  );
}
