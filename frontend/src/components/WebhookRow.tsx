import { faList, faPaperPlane, faPen, faTrash } from '@fortawesome/free-solid-svg-icons';
import { TableData, TableRow } from '@/elements/data-display/Table.tsx';
import ContextMenu, { ContextMenuToggle } from '@/elements/overlays/ContextMenu.tsx';
import FormattedTimestamp from '@/elements/time/FormattedTimestamp.tsx';
import Text from '@/elements/typography/Text.tsx';
import type { Webhook, WebhookScopeApi } from '../api.ts';
import { useExtTranslations } from '../translations.ts';
import EnabledSwitch from './EnabledSwitch.tsx';
import { LastDeliveryBadge } from './StatusBadge.tsx';

export default function WebhookRow({
  api,
  webhook,
  canUpdate,
  canDelete,
  serverFilterEmptyLabel,
  onEdit,
  onDelete,
  onDeliveries,
  onTest,
  onChanged,
}: {
  api: WebhookScopeApi;
  webhook: Webhook;
  canUpdate: boolean;
  canDelete: boolean;
  // shows the server filter column, with this label for an empty filter
  serverFilterEmptyLabel: string | null;
  onEdit: () => void;
  onDelete: () => void;
  onDeliveries: () => void;
  onTest: () => void;
  onChanged: () => void;
}) {
  const { t: tExt } = useExtTranslations();

  const filterNames = webhook.serverFilter.map((server) => server.name).join(', ');

  return (
    <ContextMenu
      items={[
        {
          type: 'action',
          icon: faPen,
          label: tExt('common.edit', {}),
          onClick: onEdit,
          color: 'gray',
          canAccess: canUpdate,
        },
        {
          type: 'action',
          icon: faPaperPlane,
          label: tExt('common.test', {}),
          onClick: onTest,
          color: 'gray',
          canAccess: canUpdate,
        },
        {
          type: 'action',
          icon: faList,
          label: tExt('common.deliveries', {}),
          onClick: onDeliveries,
          color: 'gray',
        },
        {
          type: 'action',
          icon: faTrash,
          label: tExt('common.delete', {}),
          onClick: onDelete,
          color: 'red',
          canAccess: canDelete,
        },
      ]}
    >
      {({ items, openMenu }) => (
        <TableRow
          onContextMenu={(e) => {
            e.preventDefault();
            openMenu(e.clientX, e.clientY);
          }}
        >
          <TableData>
            <Text size='sm' fw={500}>
              {webhook.name}
            </Text>
            <Text size='xs' c='dimmed' className='max-w-80 truncate'>
              {webhook.url}
            </Text>
          </TableData>
          <TableData>{tExt(`format.${webhook.format}`, {})}</TableData>
          <TableData>{webhook.events.length}</TableData>
          {serverFilterEmptyLabel !== null && (
            <TableData>
              {filterNames ? (
                <Text size='sm' className='max-w-60 truncate' title={filterNames}>
                  {filterNames}
                </Text>
              ) : (
                <Text size='sm' c='dimmed'>
                  {serverFilterEmptyLabel}
                </Text>
              )}
            </TableData>
          )}
          <TableData>
            <EnabledSwitch api={api} webhook={webhook} disabled={!canUpdate} onChanged={onChanged} />
          </TableData>
          <TableData>
            <LastDeliveryBadge lastDelivery={webhook.lastDelivery} />
          </TableData>
          <TableData>
            <FormattedTimestamp timestamp={webhook.created} />
          </TableData>
          <ContextMenuToggle items={items} openMenu={openMenu} />
        </TableRow>
      )}
    </ContextMenu>
  );
}
