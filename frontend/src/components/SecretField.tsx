import { faCopy, faRotate } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useState } from 'react';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Button from '@/elements/buttons/Button.tsx';
import PasswordInput from '@/elements/input/PasswordInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import { handleCopyToClipboard } from '@/lib/clipboard/copy.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import { useExtTranslations } from '../translations.ts';

export default function SecretField({
  secret,
  onRotate,
  canRotate,
}: {
  secret: string;
  onRotate: () => Promise<void>;
  canRotate: boolean;
}) {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();

  const [confirmOpen, setConfirmOpen] = useState(false);

  const doRotate = async () => {
    await onRotate();
    addToast(tExt('form.rotated', {}), 'success');
    setConfirmOpen(false);
  };

  return (
    <>
      <ConfirmationModal
        opened={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        title={tExt('form.rotateTitle', {})}
        confirm={tExt('form.rotateSecret', {})}
        onConfirmed={doRotate}
      >
        {tExt('form.rotateContent', {})}
      </ConfirmationModal>

      <Group gap='xs' align='flex-end' wrap='nowrap'>
        <PasswordInput
          label={tExt('form.secret', {})}
          description={tExt('form.secretDescription', {})}
          value={secret}
          readOnly
          ff='monospace'
          className='flex-1'
        />
        <Tooltip label={tExt('form.copy', {})}>
          <ActionIcon size='lg' variant='default' onClick={handleCopyToClipboard(secret, addToast)}>
            <FontAwesomeIcon icon={faCopy} />
          </ActionIcon>
        </Tooltip>
        {canRotate && (
          <Button
            variant='default'
            leftSection={<FontAwesomeIcon icon={faRotate} />}
            onClick={() => setConfirmOpen(true)}
          >
            {tExt('form.rotateSecret', {})}
          </Button>
        )}
      </Group>
    </>
  );
}
