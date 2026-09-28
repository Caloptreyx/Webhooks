import { Center } from '@mantine/core';
import type { UseQueryResult } from '@tanstack/react-query';
import { useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Table from '@/elements/data-display/Table.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import { Modal } from '@/elements/modals/Modal.tsx';
import Text from '@/elements/typography/Text.tsx';
import type { ClientWebhooks, Webhook, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import DeliveriesTable from './DeliveriesTable.tsx';
import useWebhookTest from './useWebhookTest.tsx';
import WebhookDeleteModal from './WebhookDeleteModal.tsx';
import WebhookModal, { type ServerFilterPicker } from './WebhookModal.tsx';
import WebhookRow from './WebhookRow.tsx';

type OpenModal =
  | { kind: 'edit'; webhook: Webhook }
  | { kind: 'delete'; webhook: Webhook }
  | { kind: 'deliveries'; webhook: Webhook }
  | null;

// the webhooks of a server or of the current account with their row actions; the page owns the create button
export default function ClientWebhookList({
  api,
  query,
  creating,
  onCreateClose,
  canUpdate,
  canDelete,
  serverFilterPicker,
  disabledText,
  emptyText,
}: {
  api: WebhookScopeApi;
  query: UseQueryResult<ClientWebhooks>;
  creating: boolean;
  onCreateClose: () => void;
  canUpdate: boolean;
  canDelete: boolean;
  serverFilterPicker: ServerFilterPicker | null;
  disabledText: string;
  emptyText: string;
}) {
  const { t: tExt } = useExtTranslations();
  const { data, isLoading, error, refetch } = query;

  const [openModal, setOpenModal] = useState<OpenModal>(null);

  const test = useWebhookTest({ api, events: data?.events ?? [], canRedeliver: canUpdate, onTested: refetch });

  const count = data?.webhooks.length ?? 0;

  return (
    <Stack gap='md'>
      {data && !data.enabled && <Alert color='yellow'>{disabledText}</Alert>}

      {data && (
        <WebhookModal
          opened={creating || openModal?.kind === 'edit'}
          onClose={() => {
            onCreateClose();
            setOpenModal(null);
          }}
          api={api}
          webhook={openModal?.kind === 'edit' ? openModal.webhook : undefined}
          events={data.events}
          defaultTemplates={data.defaultTemplates}
          serverFilterPicker={serverFilterPicker}
          canRotateSecret={canUpdate}
          onSaved={refetch}
        />
      )}
      <WebhookDeleteModal
        api={api}
        webhook={openModal?.kind === 'delete' ? openModal.webhook : null}
        onClose={() => setOpenModal(null)}
        onDeleted={refetch}
      />
      <Modal
        size='xl'
        opened={openModal?.kind === 'deliveries'}
        onClose={() => setOpenModal(null)}
        title={
          openModal?.kind === 'deliveries'
            ? tExt('common.deliveriesTitle', { name: openModal.webhook.name })
            : undefined
        }
      >
        {openModal?.kind === 'deliveries' && (
          <DeliveriesTable
            api={api}
            filters={{ webhook: openModal.webhook.uuid }}
            events={data?.events ?? []}
            showWebhook={false}
            canRedeliver={canUpdate}
          />
        )}
      </Modal>
      {test.modal}

      <Table
        columns={[
          tExt('table.name', {}),
          tExt('table.format', {}),
          tExt('table.events', {}),
          ...(serverFilterPicker ? [tExt('table.servers', {})] : []),
          tExt('table.enabled', {}),
          tExt('table.lastDelivery', {}),
          tExt('table.created', {}),
          '',
        ]}
        loading={isLoading}
        error={error ? httpErrorToHuman(error) : null}
        pagination={{ total: count, perPage: Math.max(count, 1), page: 1, data: data?.webhooks ?? [] }}
        empty={
          <Center py='lg'>
            <Text c='dimmed'>{emptyText}</Text>
          </Center>
        }
      >
        {data?.webhooks.map((webhook) => (
          <WebhookRow
            key={webhook.uuid}
            api={api}
            webhook={webhook}
            canUpdate={canUpdate}
            canDelete={canDelete}
            serverFilterEmptyLabel={serverFilterPicker?.emptyLabel ?? null}
            onEdit={() => setOpenModal({ kind: 'edit', webhook })}
            onDelete={() => setOpenModal({ kind: 'delete', webhook })}
            onDeliveries={() => setOpenModal({ kind: 'deliveries', webhook })}
            onTest={() => test.run(webhook)}
            onChanged={refetch}
          />
        ))}
      </Table>
    </Stack>
  );
}
