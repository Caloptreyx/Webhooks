import { faPlus } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import Button from '@/elements/buttons/Button.tsx';
import ConditionalTooltip from '@/elements/overlays/ConditionalTooltip.tsx';
import type { ClientWebhooks } from '../api.ts';
import { useExtTranslations } from '../translations.ts';

// create button of the server and account pages, disabled once the limit is reached
export default function CreateWebhookButton({
  webhooks,
  limitReachedText,
  onClick,
}: {
  webhooks: ClientWebhooks;
  limitReachedText: string;
  onClick: () => void;
}) {
  const { t: tExt } = useExtTranslations();

  const limitReached = webhooks.webhooks.length >= webhooks.limit;

  return (
    <ConditionalTooltip enabled={limitReached} label={limitReachedText}>
      <Button disabled={limitReached} onClick={onClick} color='blue' leftSection={<FontAwesomeIcon icon={faPlus} />}>
        {tExt('common.createWebhook', {})}
      </Button>
    </ConditionalTooltip>
  );
}
