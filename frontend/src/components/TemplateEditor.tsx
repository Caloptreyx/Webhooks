import { faChevronDown, faEye } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Input, UnstyledButton } from '@mantine/core';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import Card from '@/elements/data-display/Card.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Select from '@/elements/input/Select.tsx';
import TextArea from '@/elements/input/TextArea.tsx';
import Collapse from '@/elements/layout/Collapse.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import {
  type DefaultTemplates,
  type EventInfo,
  MAX_BODY_TEMPLATE,
  type PreviewRequest,
  type WebhookFormat,
} from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import JsonCode from './JsonCode.tsx';

export const DEFAULT_JSON_TEMPLATE = `{
  "event": "{{ event }}",
  "summary": "{{ summary }}",
  "server": {{ server|tojson }},
  "data": {{ data|tojson }}
}`;

const PLACEHOLDERS = [
  ['{{ event }}', 'event'],
  ['{{ event_label }}', 'event_label'],
  ['{{ summary }}', 'summary'],
  ['{{ severity }}', 'severity'],
  ['{{ color }}', 'color'],
  ['{{ timestamp }}', 'timestamp'],
  ['{{ panel.name }}', 'panel_name'],
  ['{{ panel.url }}', 'panel_url'],
  ['{{ server.name }}', 'server_name'],
  ['{{ server.uuid }}, {{ server.uuid_short }}', 'server_uuid'],
  ['{{ server.url }}', 'server_url'],
  ['{{ server.node.name }}', 'server_node'],
  ['{{ server.owner.username }}', 'server_owner'],
  ['{{ node.uuid }}, {{ node.name }}', 'node'],
  ['{{ user.uuid }}, {{ user.username }}', 'user'],
  ['{{ data }}, {{ data.exit_code }}', 'data'],
] as const;

type PreviewResult = { body: string; error: null } | { body: null; error: string };

export default function TemplateEditor({
  format,
  value,
  onChange,
  defaultTemplates,
  events,
  selectedEvents,
  preview,
  disabled,
}: {
  format: WebhookFormat;
  value: string;
  onChange: (template: string) => void;
  defaultTemplates: DefaultTemplates;
  events: EventInfo[];
  selectedEvents: string[];
  preview: (request: PreviewRequest) => Promise<string>;
  disabled?: boolean;
}) {
  const { t: tExt, tReact: tExtReact } = useExtTranslations();

  const [helpOpen, setHelpOpen] = useState(false);
  const [previewEvent, setPreviewEvent] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [result, setResult] = useState<PreviewResult | null>(null);

  // preview among the selected events, or any available event while nothing is selected yet
  const previewOptions = (
    selectedEvents.length > 0 ? events.filter((event) => selectedEvents.includes(event.id)) : events
  ).map((event) => ({ value: event.id, label: event.label }));

  useEffect(() => {
    if (!previewEvent || !previewOptions.some((option) => option.value === previewEvent)) {
      setPreviewEvent(previewOptions[0]?.value ?? null);
    }
  }, [previewOptions.map((option) => option.value).join(',')]);

  useEffect(() => {
    setResult(null);
  }, [format, value]);

  const doPreview = async () => {
    if (!previewEvent) return;
    setPreviewing(true);
    try {
      const body = await preview({ format, bodyTemplate: value.trim() ? value : null, event: previewEvent });
      setResult({ body, error: null });
    } catch (msg) {
      setResult({ body: null, error: httpErrorToHuman(msg) });
    }
    setPreviewing(false);
  };

  return (
    <Stack gap='xs'>
      <TextArea
        label={tExt('form.bodyTemplate', {})}
        description={tExt('form.bodyTemplateDescription', {})}
        placeholder={tExt('form.bodyTemplatePlaceholder', {})}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        error={
          value.length > MAX_BODY_TEMPLATE ? tExt('form.bodyTemplateTooLong', { max: MAX_BODY_TEMPLATE }) : undefined
        }
        disabled={disabled}
        autosize
        minRows={6}
        maxRows={20}
        spellCheck={false}
        styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
      />

      <Group justify='space-between' align='flex-end'>
        <Group gap='xs'>
          <Button
            size='xs'
            variant='default'
            onClick={() => onChange(format === 'discord' ? defaultTemplates.discord : DEFAULT_JSON_TEMPLATE)}
            disabled={disabled}
          >
            {tExt('form.useDefaultTemplate', {})}
          </Button>
          <Button size='xs' variant='default' onClick={() => onChange('')} disabled={disabled || !value}>
            {tExt('form.clearTemplate', {})}
          </Button>
        </Group>
        <Group gap='xs' align='flex-end'>
          <Select
            size='xs'
            aria-label={tExt('form.previewEvent', {})}
            placeholder={tExt('form.previewEvent', {})}
            data={previewOptions}
            value={previewEvent}
            onChange={setPreviewEvent}
            allowDeselect={false}
            searchable
            w={220}
          />
          <Button
            size='xs'
            leftSection={<FontAwesomeIcon icon={faEye} />}
            onClick={doPreview}
            loading={previewing}
            disabled={!previewEvent}
          >
            {tExt('form.preview', {})}
          </Button>
        </Group>
      </Group>

      {result?.error && (
        <Alert color='red' title={tExt('form.previewError', {})}>
          <Text size='sm' className='whitespace-pre-wrap'>
            {result.error}
          </Text>
        </Alert>
      )}
      {result?.body != null && (
        <Input.Wrapper label={tExt('form.previewResult', {})}>
          <JsonCode value={result.body} />
        </Input.Wrapper>
      )}

      <Card withBorder p='xs'>
        <UnstyledButton onClick={() => setHelpOpen(!helpOpen)} className='w-full'>
          <Group justify='space-between'>
            <Text size='sm' fw={500}>
              {tExt('help.title', {})}
            </Text>
            <FontAwesomeIcon
              icon={faChevronDown}
              size='xs'
              style={{ transform: helpOpen ? 'rotate(0deg)' : 'rotate(-90deg)', transition: 'transform 0.15s ease' }}
            />
          </Group>
        </UnstyledButton>
        <Collapse expanded={helpOpen}>
          <Stack gap='xs' pt='xs'>
            <Text size='sm' c='dimmed'>
              {tExtReact('help.intro', {
                example: <Code>{'{{ server.name }}'}</Code>,
                tojson: <Code>{'{{ data|tojson }}'}</Code>,
              })}
            </Text>
            {PLACEHOLDERS.map(([placeholder, key]) => (
              <Group key={key} gap='sm' wrap='nowrap' align='flex-start'>
                <Code className='shrink-0'>{placeholder}</Code>
                <Text size='sm'>{tExt(`help.fields.${key}`, {})}</Text>
              </Group>
            ))}
          </Stack>
        </Collapse>
      </Card>
    </Stack>
  );
}
