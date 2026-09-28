import Badge from '@/elements/data-display/Badge.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import FormattedTimestamp from '@/elements/time/FormattedTimestamp.tsx';
import Text from '@/elements/typography/Text.tsx';
import type { DeliveryStatus, LastDelivery } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

const COLORS: Record<DeliveryStatus, string> = {
  pending: 'yellow',
  success: 'green',
  failed: 'red',
};

export function DeliveryStatusBadge({ status }: { status: DeliveryStatus }) {
  const { t: tExt } = useExtTranslations();

  return <Badge color={COLORS[status]}>{tExt(`status.${status}`, {})}</Badge>;
}

export function LastDeliveryBadge({ lastDelivery }: { lastDelivery: LastDelivery | null }) {
  const { t: tExt } = useExtTranslations();

  if (!lastDelivery) {
    return (
      <Text size='sm' c='dimmed'>
        {tExt('lastDelivery.never', {})}
      </Text>
    );
  }

  return (
    <Tooltip
      label={
        lastDelivery.responseStatus !== null
          ? tExt('lastDelivery.tooltip', { status: lastDelivery.responseStatus })
          : tExt('lastDelivery.tooltipNoResponse', {})
      }
    >
      <div>
        <DeliveryStatusBadge status={lastDelivery.status} />
        <Text size='xs' c='dimmed'>
          <FormattedTimestamp timestamp={lastDelivery.created} withTooltip={false} />
        </Text>
      </div>
    </Tooltip>
  );
}
