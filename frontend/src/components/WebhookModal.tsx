import { ModalProps, SimpleGrid } from '@mantine/core';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Select from '@/elements/input/Select.tsx';
import ServerMultiSelect from '@/elements/input/ServerMultiSelect.tsx';
import Switch from '@/elements/input/Switch.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Divider from '@/elements/layout/Divider.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import FormModal from '@/elements/modals/FormModal.tsx';
import { ModalFooter } from '@/elements/modals/Modal.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useTranslations } from '@/providers/TranslationProvider.tsx';
import {
  type DefaultTemplates,
  type EventInfo,
  MAX_BODY_TEMPLATE,
  MAX_EVENTS,
  MAX_SERVER_FILTER,
  type Webhook,
  type WebhookFormat,
  type WebhookHeader,
  type WebhookPatch,
  type WebhookScopeApi,
  type WebhookServerRef,
} from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import EventPicker from './EventPicker.tsx';
import HeadersEditor, { headerNameError } from './HeadersEditor.tsx';
import SecretField from './SecretField.tsx';
import TemplateEditor from './TemplateEditor.tsx';

// texts of the server filter picker; its servers come from `api.filterServers`
export interface ServerFilterPicker {
  description: string;
  // what an empty selection means, e.g. "All my servers"
  emptyLabel: string;
}

function isValidUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return (url.protocol === 'http:' || url.protocol === 'https:') && !!url.hostname;
  } catch {
    return false;
  }
}

export default function WebhookModal({
  api,
  webhook,
  events,
  defaultTemplates,
  serverFilterPicker,
  canRotateSecret,
  onSaved,
  ...props
}: Omit<ModalProps, 'children' | 'title'> & {
  api: WebhookScopeApi;
  webhook?: Webhook;
  events: EventInfo[];
  defaultTemplates: DefaultTemplates;
  serverFilterPicker: ServerFilterPicker | null;
  canRotateSecret: boolean;
  onSaved: () => void;
}) {
  const { t: tExt } = useExtTranslations();
  const { t } = useTranslations();
  const { addToast } = useToast();

  const [loading, setLoading] = useState(false);
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [format, setFormat] = useState<WebhookFormat>('discord');
  const [enabled, setEnabled] = useState(true);
  const [selectedEvents, setSelectedEvents] = useState<string[]>([]);
  const [eventsTouched, setEventsTouched] = useState(false);
  const [serverFilter, setServerFilter] = useState<WebhookServerRef[]>([]);
  const [headers, setHeaders] = useState<WebhookHeader[]>([]);
  const [bodyTemplate, setBodyTemplate] = useState('');
  const [secret, setSecret] = useState<string | null>(null);

  useEffect(() => {
    if (!props.opened) return;
    const known = new Set(events.map((event) => event.id));
    setName(webhook?.name ?? '');
    setUrl(webhook?.url ?? '');
    setFormat(webhook?.format ?? 'discord');
    setEnabled(webhook?.enabled ?? true);
    setSelectedEvents(webhook?.events.filter((id) => known.has(id)) ?? []);
    setEventsTouched(false);
    setServerFilter(webhook?.serverFilter ?? []);
    setHeaders(webhook?.headers ?? []);
    setBodyTemplate(webhook?.bodyTemplate ?? '');
    setSecret(webhook?.secret ?? null);
  }, [props.opened]);

  const filterServers = serverFilterPicker ? api.filterServers : undefined;
  // redacted webhooks hide their url and header values; they are neither shown nor sent back
  const redacted = !!webhook?.redacted;
  const cleanedHeaders = headers.filter((header) => header.name.trim() || header.value);
  const headersValid = cleanedHeaders.every(
    (header, index) => header.name.trim() && !headerNameError(cleanedHeaders, index),
  );
  const urlValid = redacted || isValidUrl(url.trim());
  const eventsError =
    selectedEvents.length === 0
      ? eventsTouched
        ? tExt('form.eventsRequired', {})
        : null
      : selectedEvents.length > MAX_EVENTS
        ? tExt('form.tooManyEvents', { max: MAX_EVENTS })
        : null;
  const valid =
    name.trim().length > 0 &&
    urlValid &&
    selectedEvents.length > 0 &&
    !eventsError &&
    headersValid &&
    bodyTemplate.length <= MAX_BODY_TEMPLATE &&
    serverFilter.length <= MAX_SERVER_FILTER;

  const doSubmit = async () => {
    setLoading(true);
    const payload: WebhookPatch = {
      name: name.trim(),
      format,
      events: selectedEvents,
      enabled,
      bodyTemplate: bodyTemplate.trim() ? bodyTemplate : null,
      serverFilter: filterServers ? serverFilter.map((server) => server.uuid) : undefined,
      ...(redacted
        ? {}
        : {
            url: url.trim(),
            headers: cleanedHeaders.map((header) => ({ name: header.name.trim(), value: header.value })),
          }),
    };

    try {
      if (webhook) {
        await api.updateWebhook(webhook.uuid, payload);
        addToast(tExt('common.updated', {}), 'success');
      } else {
        await api.createWebhook({
          name: name.trim(),
          url: url.trim(),
          format,
          events: selectedEvents,
          enabled,
          bodyTemplate: payload.bodyTemplate ?? null,
          serverFilter: payload.serverFilter,
          headers: payload.headers ?? [],
        });
        addToast(tExt('common.created', {}), 'success');
      }
      onSaved();
      props.onClose();
    } catch (msg) {
      addToast(httpErrorToHuman(msg), 'error');
    }
    setLoading(false);
  };

  const doRotate = async () => {
    if (!webhook) return;
    setSecret(await api.rotateSecret(webhook.uuid));
    onSaved();
  };

  return (
    <FormModal
      size='xl'
      loading={loading}
      {...props}
      title={webhook ? tExt('form.editTitle', { name: webhook.name }) : tExt('form.createTitle', {})}
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) doSubmit();
      }}
    >
      <Stack gap='md'>
        <SimpleGrid cols={{ base: 1, sm: 2 }}>
          <TextInput
            withAsterisk
            label={tExt('form.name', {})}
            placeholder={tExt('form.namePlaceholder', {})}
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={255}
          />
          <Select
            label={tExt('form.format', {})}
            description={tExt('form.formatDescription', {})}
            data={[
              { value: 'discord', label: tExt('format.discord', {}) },
              { value: 'json', label: tExt('format.json', {}) },
            ]}
            value={format}
            onChange={(value) => value && setFormat(value as WebhookFormat)}
          />
        </SimpleGrid>

        {redacted ? (
          <Alert color='gray'>{tExt('form.urlHidden', {})}</Alert>
        ) : (
          <TextInput
            withAsterisk
            label={tExt('form.url', {})}
            description={tExt('form.urlDescription', {})}
            placeholder='https://discord.com/api/webhooks/...'
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            error={url.length > 0 && !urlValid ? tExt('form.invalidUrl', {}) : undefined}
            maxLength={2048}
          />
        )}

        <Switch
          label={tExt('form.enabled', {})}
          description={tExt('form.enabledDescription', {})}
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
        />

        {webhook && secret && (
          <SecretField secret={secret} canRotate={canRotateSecret && !redacted} onRotate={doRotate} />
        )}
        {!webhook && (
          <Text size='sm' c='dimmed'>
            {tExt('form.secretCreated', {})}
          </Text>
        )}

        <Divider />

        <EventPicker
          events={events}
          value={selectedEvents}
          onChange={(value) => {
            setSelectedEvents(value);
            setEventsTouched(true);
          }}
          error={eventsError}
        />

        {serverFilterPicker && filterServers && (
          <ServerMultiSelect<WebhookServerRef>
            label={tExt('form.serverFilter', {})}
            description={serverFilterPicker.description}
            // the placeholder spells out what an empty selection means
            placeholder={serverFilter.length ? '' : serverFilterPicker.emptyLabel}
            queryKey={[...api.queryKey, 'filter-servers']}
            fetcher={filterServers}
            value={serverFilter}
            onChange={setServerFilter}
          />
        )}

        {!redacted && <HeadersEditor value={headers} onChange={setHeaders} />}

        <Divider />

        <TemplateEditor
          format={format}
          value={bodyTemplate}
          onChange={setBodyTemplate}
          defaultTemplates={defaultTemplates}
          events={events}
          selectedEvents={selectedEvents}
          preview={api.preview}
        />

        <ModalFooter>
          <Button type='submit' loading={loading} disabled={!valid}>
            {webhook ? t('common.button.save', {}) : t('common.button.create', {})}
          </Button>
          <Button variant='default' onClick={props.onClose}>
            {t('common.button.cancel', {})}
          </Button>
        </ModalFooter>
      </Stack>
    </FormModal>
  );
}
