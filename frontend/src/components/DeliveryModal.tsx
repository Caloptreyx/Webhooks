import { faRotateRight } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Input, SimpleGrid } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import { Modal, ModalFooter } from '@/elements/modals/Modal.tsx';
import FormattedTimestamp from '@/elements/time/FormattedTimestamp.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useTranslations } from '@/providers/TranslationProvider.tsx';
import type { Delivery, DeliverySummary, EventInfo, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import JsonCode from './JsonCode.tsx';
import { DeliveryStatusBadge } from './StatusBadge.tsx';

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <Text size='xs' c='dimmed'>
        {label}
      </Text>
      <div>{children}</div>
    </div>
  );
}

export default function DeliveryModal({
  api,
  delivery,
  events,
  title,
  canRedeliver,
  onRedelivered,
  onClose,
}: {
  api: WebhookScopeApi;
  // a full delivery (e.g. a test result) is shown as is, a summary is loaded first
  delivery: Delivery | DeliverySummary | null;
  events: EventInfo[];
  title?: string;
  canRedeliver: boolean;
  onRedelivered?: () => void;
  onClose: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { t } = useTranslations();
  const { addToast } = useToast();

  const [redelivering, setRedelivering] = useState(false);

  const needsFetch = !!delivery && !('payload' in delivery);
  const { data, isLoading, error } = useQuery({
    queryKey: [...api.queryKey, 'delivery', delivery?.uuid],
    queryFn: () => api.getDelivery(delivery as DeliverySummary),
    enabled: needsFetch,
  });

  const full: Delivery | undefined = delivery && 'payload' in delivery ? delivery : data;
  const eventLabel = (id: string) => events.find((event) => event.id === id)?.label ?? id;

  const doRedeliver = async () => {
    if (!delivery) return;
    setRedelivering(true);
    try {
      await api.redeliver(delivery);
      addToast(tExt('deliveries.redelivered', {}), 'success');
      onRedelivered?.();
      onClose();
    } catch (msg) {
      addToast(httpErrorToHuman(msg), 'error');
    }
    setRedelivering(false);
  };

  return (
    <Modal
      size='xl'
      opened={!!delivery}
      onClose={onClose}
      title={title ?? (delivery ? tExt('deliveries.detailsTitle', { event: eventLabel(delivery.event) }) : '')}
    >
      {needsFetch && isLoading ? (
        <Spinner.Centered />
      ) : error || !full ? (
        <Alert color='red'>{error ? httpErrorToHuman(error) : null}</Alert>
      ) : (
        <Stack gap='md'>
          <SimpleGrid cols={{ base: 2, sm: 4 }} spacing='md'>
            <Field label={tExt('table.status', {})}>
              <DeliveryStatusBadge status={full.status} />
            </Field>
            <Field label={tExt('table.event', {})}>
              <Text size='sm'>{eventLabel(full.event)}</Text>
              <Code>{full.event}</Code>
            </Field>
            <Field label={tExt('table.webhook', {})}>
              <Text size='sm'>{full.webhookName}</Text>
            </Field>
            <Field label={tExt('table.response', {})}>
              <Text size='sm'>
                {full.responseStatus !== null ? `HTTP ${full.responseStatus}` : tExt('deliveries.noResponse', {})}
              </Text>
            </Field>
            <Field label={tExt('table.attempts', {})}>
              <Text size='sm'>{full.attempts}</Text>
            </Field>
            <Field label={tExt('table.duration', {})}>
              <Text size='sm'>
                {full.durationMs !== null ? tExt('table.durationMs', { ms: full.durationMs }) : '-'}
              </Text>
            </Field>
            <Field label={tExt('table.created', {})}>
              <Text size='sm'>
                <FormattedTimestamp timestamp={full.created} />
              </Text>
            </Field>
            {full.status === 'pending' ? (
              <Field label={tExt('deliveries.nextAttempt', {})}>
                <Text size='sm'>
                  <FormattedTimestamp timestamp={full.nextAttempt} showNA />
                </Text>
              </Field>
            ) : (
              <Field label={tExt('deliveries.completed', {})}>
                <Text size='sm'>
                  <FormattedTimestamp timestamp={full.completed} showNA />
                </Text>
              </Field>
            )}
          </SimpleGrid>

          {full.error && (
            <Alert color='red' title={tExt('deliveries.error', {})}>
              <Text size='sm' className='whitespace-pre-wrap break-all'>
                {full.error}
              </Text>
            </Alert>
          )}

          <Input.Wrapper label={tExt('deliveries.payload', {})}>
            <JsonCode value={full.payload} />
          </Input.Wrapper>
          <Input.Wrapper label={tExt('deliveries.requestBody', {})}>
            {full.requestBody ? (
              <JsonCode value={full.requestBody} />
            ) : (
              <Text size='sm' c='dimmed'>
                {tExt('deliveries.noBody', {})}
              </Text>
            )}
          </Input.Wrapper>
          <Input.Wrapper label={tExt('deliveries.responseBody', {})}>
            {full.responseBody ? (
              <JsonCode value={full.responseBody} />
            ) : (
              <Text size='sm' c='dimmed'>
                {tExt('deliveries.noBody', {})}
              </Text>
            )}
          </Input.Wrapper>
        </Stack>
      )}

      <ModalFooter>
        {canRedeliver && full && (
          <Button leftSection={<FontAwesomeIcon icon={faRotateRight} />} onClick={doRedeliver} loading={redelivering}>
            {tExt('deliveries.redeliver', {})}
          </Button>
        )}
        <Button variant='default' onClick={onClose}>
          {t('common.button.close', {})}
        </Button>
      </ModalFooter>
    </Modal>
  );
}
