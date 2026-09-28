import { faBell } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { Extension, ExtensionContext } from 'shared';
import AccountWebhooksPage from './pages/account/AccountWebhooksPage.tsx';
import AdminConfigurationPage from './pages/admin/AdminConfigurationPage.tsx';
import ServerWebhooksPage from './pages/server/ServerWebhooksPage.tsx';
import { getExtTranslations } from './translations.ts';

class CaloptreyxWebhooksExtension extends Extension {
  public cardConfigurationPage: React.FC | null = AdminConfigurationPage;
  public cardIcon: React.ReactNode = <FontAwesomeIcon icon={faBell} />;

  public initialize(ctx: ExtensionContext): void {
    ctx.extensionRegistry.enterRoutes((routes) =>
      routes
        .addServerRoute({
          name: () => getExtTranslations().t('common.webhooks', {}),
          icon: faBell,
          path: '/webhooks',
          element: ServerWebhooksPage,
          permission: 'webhooks.read',
        })
        .addAccountRoute({
          name: () => getExtTranslations().t('common.webhooks', {}),
          icon: faBell,
          path: '/webhooks',
          element: AccountWebhooksPage,
        }),
    );

    ctx.extensionRegistry.enterPermissionIcons((icons) =>
      icons
        .addServerPermissionIcon('webhooks', <FontAwesomeIcon icon={faBell} />)
        .addAdminPermissionIcon('webhooks', <FontAwesomeIcon icon={faBell} />),
    );
  }
}

export default new CaloptreyxWebhooksExtension();
