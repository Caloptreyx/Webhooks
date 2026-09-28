import { faPlus, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Input } from '@mantine/core';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import { MAX_HEADERS, type WebhookHeader } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

const HEADER_NAME_REGEX = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const RESERVED_HEADERS = ['host', 'content-length'];

// returns the translation key of the problem with a header name, or null when it is valid
export function headerNameError(
  headers: WebhookHeader[],
  index: number,
): 'form.headerInvalid' | 'form.headerReserved' | 'form.headerDuplicate' | null {
  const name = headers[index].name.trim().toLowerCase();
  if (!name) return null;
  if (!HEADER_NAME_REGEX.test(name)) return 'form.headerInvalid';
  if (RESERVED_HEADERS.includes(name) || name.startsWith('x-webhook-')) return 'form.headerReserved';
  if (headers.some((header, i) => i < index && header.name.trim().toLowerCase() === name)) {
    return 'form.headerDuplicate';
  }
  return null;
}

export default function HeadersEditor({
  value,
  onChange,
  disabled,
}: {
  value: WebhookHeader[];
  onChange: (headers: WebhookHeader[]) => void;
  disabled?: boolean;
}) {
  const { t: tExt } = useExtTranslations();

  const update = (index: number, patch: Partial<WebhookHeader>) =>
    onChange(value.map((header, i) => (i === index ? { ...header, ...patch } : header)));

  return (
    <Input.Wrapper label={tExt('form.headers', {})} description={tExt('form.headersDescription', {})}>
      <Stack gap='xs' mt='xs'>
        {value.map((header, index) => {
          const error = headerNameError(value, index);

          return (
            <Group key={index} gap='xs' align='flex-start' wrap='nowrap'>
              <TextInput
                placeholder={tExt('form.headerName', {})}
                value={header.name}
                onChange={(e) => update(index, { name: e.target.value })}
                error={error ? tExt(error, {}) : undefined}
                disabled={disabled}
                ff='monospace'
                className='flex-1'
              />
              <TextInput
                placeholder={tExt('form.headerValue', {})}
                value={header.value}
                onChange={(e) => update(index, { value: e.target.value })}
                disabled={disabled}
                ff='monospace'
                className='flex-[1.5]'
              />
              <ActionIcon
                size='lg'
                variant='subtle'
                color='red'
                onClick={() => onChange(value.filter((_, i) => i !== index))}
                disabled={disabled}
                aria-label={tExt('common.delete', {})}
              >
                <FontAwesomeIcon icon={faTrash} />
              </ActionIcon>
            </Group>
          );
        })}
        <Group>
          <Button
            size='xs'
            variant='default'
            leftSection={<FontAwesomeIcon icon={faPlus} />}
            onClick={() => onChange([...value, { name: '', value: '' }])}
            disabled={disabled || value.length >= MAX_HEADERS}
          >
            {tExt('form.addHeader', {})}
          </Button>
        </Group>
      </Stack>
    </Input.Wrapper>
  );
}
