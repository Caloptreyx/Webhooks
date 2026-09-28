import { faList, faPaperPlane, faPen, faPlus, faSearch, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Center } from '@mantine/core';
import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import { AdminCan } from '@/elements/Can.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import Table, { TableData, TableRow } from '@/elements/data-display/Table.tsx';
import TableLink from '@/elements/data-display/TableLink.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import SegmentedControl from '@/elements/layout/SegmentedControl.tsx';
import ContextMenu, { ContextMenuToggle } from '@/elements/overlays/ContextMenu.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useSearchablePaginatedTable } from '@/plugins/resource/useSearchablePaginatedTable.ts';
import { useAdminCan } from '@/plugins/usePermissions.ts';
import { adminWebhookApi, getAdminWebhooks, type Webhook, type WebhookScopeFilter } from '../../api.ts';
import EnabledSwitch from '../../components/EnabledSwitch.tsx';
import { LastDeliveryBadge } from '../../components/StatusBadge.tsx';
import useWebhookTest from '../../components/useWebhookTest.tsx';
import WebhookDeleteModal from '../../components/WebhookDeleteModal.tsx';
import WebhookModal, { type ServerFilterPicker } from '../../components/WebhookModal.tsx';
import { useExtTranslations } from '../../translations.ts';
import useAdminEvents from './useAdminEvents.ts';

type OpenModal = { kind: 'create' } | { kind: 'edit'; webhook: Webhook } | { kind: 'delete'; webhook: Webhook } | null;

export default function WebhooksTab({ onViewDeliveries }: { onViewDeliveries: (webhook: Webhook) => void }) {
  const { t: tExt } = useExtTranslations();
  const canManage = useAdminCan('webhooks.manage');
  const { data: catalog, error: catalogError } = useAdminEvents();

  const [scope, setScope] = useState<WebhookScopeFilter>('all');
  const [openModal, setOpenModal] = useState<OpenModal>(null);

  const { data, loading, error, search, setSearch, setPage, refetch } = useSearchablePaginatedTable({
    queryKey: [...adminWebhookApi.queryKey, 'webhooks'],
    deps: [scope],
    fetcher: (page, search) => getAdminWebhooks(page, search, scope),
  });

  const test = useWebhookTest({
    api: adminWebhookApi,
    events: catalog?.events ?? [],
    canRedeliver: canManage,
    onTested: refetch,
  });

  const editing = openModal?.kind === 'edit' ? openModal.webhook : undefined;
  const serverOwned = !!editing?.server;
  const owner = editing?.user ?? null;
  // server and account webhooks only receive server events
  const events = (catalog?.events ?? []).filter((event) => !(serverOwned || owner) || event.serverScope);
  const serverFilterPicker: ServerFilterPicker | null = serverOwned
    ? null
    : owner
      ? {
          description: tExt('form.serverFilterOwnerDescription', { username: owner.username }),
          emptyLabel: tExt('form.allOwnerServers', { username: owner.username }),
        }
      : {
          description: tExt('form.serverFilterDescription', {}),
          emptyLabel: tExt('form.allServers', {}),
        };

  return (
    <>
      {catalog && (
        <WebhookModal
          opened={openModal?.kind === 'create' || openModal?.kind === 'edit'}
          onClose={() => setOpenModal(null)}
          api={adminWebhookApi}
          webhook={editing}
          events={events}
          defaultTemplates={catalog.defaultTemplates}
          serverFilterPicker={serverFilterPicker}
          canRotateSecret={canManage}
          onSaved={refetch}
        />
      )}
      <WebhookDeleteModal
        api={adminWebhookApi}
        webhook={openModal?.kind === 'delete' ? openModal.webhook : null}
        onClose={() => setOpenModal(null)}
        onDeleted={refetch}
      />
      {test.modal}

      {catalogError && (
        <Alert color='red' mb='md'>
          {httpErrorToHuman(catalogError)}
        </Alert>
      )}

      <Group justify='space-between' mb='md'>
        <SegmentedControl
          data={[
            { value: 'all', label: tExt('pages.admin.webhooks.scopeAll', {}) },
            { value: 'global', label: tExt('pages.admin.webhooks.scopeGlobal', {}) },
            { value: 'server', label: tExt('pages.admin.webhooks.scopeServer', {}) },
            { value: 'account', label: tExt('pages.admin.webhooks.scopeAccount', {}) },
          ]}
          value={scope}
          onChange={(value) => setScope(value as WebhookScopeFilter)}
        />
        <Group>
          <TextInput
            placeholder={tExt('pages.admin.webhooks.search', {})}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            leftSection={<FontAwesomeIcon icon={faSearch} />}
            w={260}
          />
          <AdminCan action='webhooks.manage'>
            <Button
              onClick={() => setOpenModal({ kind: 'create' })}
              disabled={!catalog}
              color='blue'
              leftSection={<FontAwesomeIcon icon={faPlus} />}
            >
              {tExt('pages.admin.webhooks.create', {})}
            </Button>
          </AdminCan>
        </Group>
      </Group>

      <Table
        columns={[
          tExt('table.name', {}),
          tExt('table.scope', {}),
          tExt('table.format', {}),
          tExt('table.events', {}),
          tExt('table.enabled', {}),
          tExt('table.lastDelivery', {}),
          '',
        ]}
        loading={loading}
        error={error}
        pagination={data}
        onPageSelect={setPage}
        empty={
          <Center py='lg'>
            <Text c='dimmed'>{tExt('pages.admin.webhooks.empty', {})}</Text>
          </Center>
        }
      >
        {data?.data.map((webhook) => (
          <ContextMenu
            key={webhook.uuid}
            items={[
              {
                type: 'action',
                icon: faPen,
                label: tExt('common.edit', {}),
                onClick: () => setOpenModal({ kind: 'edit', webhook }),
                color: 'gray',
                canAccess: canManage && !!catalog,
              },
              {
                type: 'action',
                icon: faPaperPlane,
                label: tExt('common.test', {}),
                onClick: () => test.run(webhook),
                color: 'gray',
                canAccess: canManage,
              },
              {
                type: 'action',
                icon: faList,
                label: tExt('common.deliveries', {}),
                onClick: () => onViewDeliveries(webhook),
                color: 'gray',
              },
              {
                type: 'action',
                icon: faTrash,
                label: tExt('common.delete', {}),
                onClick: () => setOpenModal({ kind: 'delete', webhook }),
                color: 'red',
                canAccess: canManage,
              },
            ]}
          >
            {({ items, openMenu }) => (
              <TableRow
                onContextMenu={(e) => {
                  e.preventDefault();
                  openMenu(e.clientX, e.clientY);
                }}
              >
                <TableData>
                  <Text size='sm' fw={500}>
                    {webhook.name}
                  </Text>
                  <Text size='xs' c='dimmed' className='max-w-80 truncate'>
                    {webhook.url}
                  </Text>
                </TableData>
                <TableData>
                  {webhook.server ? (
                    <TableLink to={`/admin/servers/${webhook.server.uuid}`}>{webhook.server.name}</TableLink>
                  ) : webhook.user ? (
                    <Group gap='xs' wrap='nowrap'>
                      <Badge color='violet'>{tExt('common.scopeAccount', {})}</Badge>
                      <TableLink to={`/admin/users/${webhook.user.uuid}`}>{webhook.user.username}</TableLink>
                    </Group>
                  ) : (
                    <Badge color='blue'>{tExt('common.scopeGlobal', {})}</Badge>
                  )}
                </TableData>
                <TableData>{tExt(`format.${webhook.format}`, {})}</TableData>
                <TableData>{webhook.events.length}</TableData>
                <TableData>
                  <EnabledSwitch api={adminWebhookApi} webhook={webhook} disabled={!canManage} onChanged={refetch} />
                </TableData>
                <TableData>
                  <LastDeliveryBadge lastDelivery={webhook.lastDelivery} />
                </TableData>
                <ContextMenuToggle items={items} openMenu={openMenu} />
              </TableRow>
            )}
          </ContextMenu>
        ))}
      </Table>
    </>
  );
}
