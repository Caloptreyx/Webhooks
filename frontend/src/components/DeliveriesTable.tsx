import { Center } from '@mantine/core';
import { useState } from 'react';
import Table, { TableData, TableRow } from '@/elements/data-display/Table.tsx';
import FormattedTimestamp from '@/elements/time/FormattedTimestamp.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useSearchablePaginatedTable } from '@/plugins/resource/useSearchablePaginatedTable.ts';
import type { DeliveryFilters, DeliverySummary, EventInfo, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import DeliveryModal from './DeliveryModal.tsx';
import { DeliveryStatusBadge } from './StatusBadge.tsx';

export default function DeliveriesTable({
  api,
  filters,
  events,
  showWebhook,
  canRedeliver,
}: {
  api: WebhookScopeApi;
  filters: DeliveryFilters;
  events: EventInfo[];
  showWebhook: boolean;
  canRedeliver: boolean;
}) {
  const { t: tExt } = useExtTranslations();

  const [selected, setSelected] = useState<DeliverySummary | null>(null);

  const { data, loading, error, setPage, refetch } = useSearchablePaginatedTable({
    queryKey: [...api.queryKey, 'deliveries'],
    deps: [filters.webhook ?? null, filters.status ?? null, filters.event ?? null],
    fetcher: (page) => api.listDeliveries(page, filters),
    modifyParams: false,
    // keep polling while deliveries are still being retried
    refetchInterval: (current) => (current?.data.some((delivery) => delivery.status === 'pending') ? 5000 : false),
  });

  const eventLabel = (id: string) => events.find((event) => event.id === id)?.label ?? id;

  return (
    <>
      <DeliveryModal
        api={api}
        delivery={selected}
        events={events}
        canRedeliver={canRedeliver}
        onRedelivered={refetch}
        onClose={() => setSelected(null)}
      />

      <Table
        columns={[
          tExt('table.status', {}),
          tExt('table.event', {}),
          ...(showWebhook ? [tExt('table.webhook', {})] : []),
          tExt('table.response', {}),
          tExt('table.duration', {}),
          tExt('table.attempts', {}),
          tExt('table.created', {}),
        ]}
        loading={loading}
        error={error}
        pagination={data}
        onPageSelect={setPage}
        empty={
          <Center py='lg'>
            <Text c='dimmed'>{tExt('deliveries.empty', {})}</Text>
          </Center>
        }
      >
        {data?.data.map((delivery) => (
          <TableRow key={delivery.uuid} className='cursor-pointer' onClick={() => setSelected(delivery)}>
            <TableData>
              <DeliveryStatusBadge status={delivery.status} />
            </TableData>
            <TableData>
              <Text size='sm'>{eventLabel(delivery.event)}</Text>
              <Code>{delivery.event}</Code>
            </TableData>
            {showWebhook && <TableData>{delivery.webhookName}</TableData>}
            <TableData>
              {delivery.responseStatus !== null ? (
                <Code>{delivery.responseStatus}</Code>
              ) : (
                <Text size='sm' c='dimmed' title={delivery.error ?? undefined}>
                  {tExt('deliveries.noResponse', {})}
                </Text>
              )}
            </TableData>
            <TableData>
              {delivery.durationMs !== null ? tExt('table.durationMs', { ms: delivery.durationMs }) : '-'}
            </TableData>
            <TableData>{delivery.attempts}</TableData>
            <TableData>
              <FormattedTimestamp timestamp={delivery.created} />
            </TableData>
          </TableRow>
        ))}
      </Table>
    </>
  );
}
