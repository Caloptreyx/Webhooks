import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { ServerCan } from '@/elements/Can.tsx';
import ServerContentContainer from '@/elements/containers/ServerContentContainer.tsx';
import { useServerCan } from '@/plugins/usePermissions.ts';
import { useServerStore } from '@/stores/server.ts';
import { serverWebhookApi } from '../../api.ts';
import ClientWebhookList from '../../components/ClientWebhookList.tsx';
import CreateWebhookButton from '../../components/CreateWebhookButton.tsx';
import { useExtTranslations } from '../../translations.ts';

export default function ServerWebhooksPage() {
  const { t: tExt } = useExtTranslations();
  const { server } = useServerStore();
  const canUpdate = useServerCan('webhooks.update');
  const canDelete = useServerCan('webhooks.delete');

  const api = useMemo(() => serverWebhookApi(server.uuid), [server.uuid]);
  const [creating, setCreating] = useState(false);

  const query = useQuery({
    queryKey: [...api.queryKey, 'webhooks'],
    queryFn: () => api.listWebhooks(),
  });
  const { data } = query;

  return (
    <ServerContentContainer
      title={tExt('common.webhooks', {})}
      subtitle={tExt('common.usage', { count: data?.webhooks.length ?? 0, limit: data?.limit ?? 0 })}
      contentRight={
        data?.enabled && (
          <ServerCan action='webhooks.create'>
            <CreateWebhookButton
              webhooks={data}
              limitReachedText={tExt('pages.server.limitReached', { limit: data.limit })}
              onClick={() => setCreating(true)}
            />
          </ServerCan>
        )
      }
    >
      <ClientWebhookList
        api={api}
        query={query}
        creating={creating}
        onCreateClose={() => setCreating(false)}
        canUpdate={canUpdate}
        canDelete={canDelete}
        serverFilterPicker={null}
        disabledText={tExt('pages.server.disabled', {})}
        emptyText={tExt('pages.server.empty', {})}
      />
    </ServerContentContainer>
  );
}
