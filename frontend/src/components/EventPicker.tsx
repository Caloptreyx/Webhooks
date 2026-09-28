import { Input, SimpleGrid } from '@mantine/core';
import { useMemo } from 'react';
import Card from '@/elements/data-display/Card.tsx';
import Checkbox from '@/elements/input/Checkbox.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { EVENT_CATEGORIES, type EventInfo, MAX_EVENTS } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

export default function EventPicker({
  events,
  value,
  onChange,
  error,
  disabled,
}: {
  events: EventInfo[];
  value: string[];
  onChange: (events: string[]) => void;
  error?: string | null;
  disabled?: boolean;
}) {
  const { t: tExt } = useExtTranslations();

  const groups = useMemo(
    () =>
      EVENT_CATEGORIES.map((category) => ({
        category,
        events: events.filter((event) => event.category === category),
      })).filter((group) => group.events.length > 0),
    [events],
  );

  const selected = new Set(value);

  const setCategory = (categoryEvents: EventInfo[], checked: boolean) => {
    const ids = new Set(categoryEvents.map((event) => event.id));
    const rest = value.filter((id) => !ids.has(id));
    onChange(checked ? [...rest, ...ids] : rest);
  };

  const toggle = (id: string, checked: boolean) =>
    onChange(checked ? [...value, id] : value.filter((selectedId) => selectedId !== id));

  return (
    <Input.Wrapper
      withAsterisk
      label={tExt('table.events', {})}
      description={tExt('form.eventsDescription', { count: value.length, max: MAX_EVENTS })}
      error={error}
    >
      <Stack gap='sm' mt='xs'>
        {groups.map(({ category, events: categoryEvents }) => {
          const count = categoryEvents.filter((event) => selected.has(event.id)).length;

          return (
            <Card key={category} withBorder p='sm'>
              <Group justify='space-between' mb='xs'>
                <Text fw={600}>{tExt(`categories.${category}`, {})}</Text>
                <Checkbox
                  size='xs'
                  labelPosition='left'
                  label={tExt('form.selectCategory', {})}
                  checked={count === categoryEvents.length}
                  indeterminate={count > 0 && count < categoryEvents.length}
                  onChange={(e) => setCategory(categoryEvents, e.target.checked)}
                  disabled={disabled}
                />
              </Group>
              <SimpleGrid cols={{ base: 1, sm: 2 }} spacing='sm' verticalSpacing='xs'>
                {categoryEvents.map((event) => (
                  <Checkbox
                    key={event.id}
                    checked={selected.has(event.id)}
                    onChange={(e) => toggle(event.id, e.target.checked)}
                    disabled={disabled}
                    label={
                      <>
                        {event.label} <Code>{event.id}</Code>
                      </>
                    }
                    description={event.description}
                  />
                ))}
              </SimpleGrid>
            </Card>
          );
        })}
      </Stack>
    </Input.Wrapper>
  );
}
