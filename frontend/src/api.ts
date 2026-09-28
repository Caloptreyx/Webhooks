import { z } from 'zod';
import getAdminServers from '@/api/admin/servers/getServers.ts';
import { axiosInstance } from '@/api/axios.ts';
import getServers from '@/api/server/getServers.ts';
import { parseFromApi, parsePaginationFromApi, serializeForApi } from '@/lib/serialization/api-transform.ts';

export const WEBHOOKS_ADMIN_BASE = '/api/admin/extensions/dev.caloptreyx.webhooks';
export const WEBHOOKS_ACCOUNT_BASE = '/api/client/extensions/dev.caloptreyx.webhooks/webhooks';
export const webhooksServerBase = (serverUuid: string) => `/api/client/servers/${serverUuid}/webhooks`;

export const MAX_EVENTS = 100;
export const MAX_HEADERS = 20;
export const MAX_SERVER_FILTER = 500;
export const MAX_BODY_TEMPLATE = 16384;

export const formatSchema = z.enum(['json', 'discord']);
export type WebhookFormat = z.infer<typeof formatSchema>;

export const deliveryStatusSchema = z.enum(['pending', 'success', 'failed']);
export type DeliveryStatus = z.infer<typeof deliveryStatusSchema>;

export const eventCategorySchema = z.enum([
  'power',
  'state',
  'install',
  'backup',
  'transfer',
  'server',
  'node',
  'user',
  'activity',
]);
export type EventCategory = z.infer<typeof eventCategorySchema>;
export const EVENT_CATEGORIES = eventCategorySchema.options;

export const eventInfoSchema = z.object({
  id: z.string(),
  label: z.string(),
  description: z.string(),
  category: eventCategorySchema,
  serverScope: z.boolean(),
});
export type EventInfo = z.infer<typeof eventInfoSchema>;

export const defaultTemplatesSchema = z.object({
  discord: z.string(),
});
export type DefaultTemplates = z.infer<typeof defaultTemplatesSchema>;

export const headerSchema = z.object({
  name: z.string(),
  value: z.string(),
});
export type WebhookHeader = z.infer<typeof headerSchema>;

export const lastDeliverySchema = z.object({
  status: deliveryStatusSchema,
  created: z.coerce.date(),
  responseStatus: z.number().int().nullable(),
});
export type LastDelivery = z.infer<typeof lastDeliverySchema>;

export const webhookSchema = z.object({
  uuid: z.string(),
  server: z
    .object({
      uuid: z.string(),
      uuidShort: z.string(),
      name: z.string(),
    })
    .nullable(),
  // owning user of an account webhook
  user: z
    .object({
      uuid: z.string(),
      username: z.string(),
    })
    .nullable(),
  name: z.string(),
  url: z.string(),
  format: formatSchema,
  events: z.array(z.string()),
  enabled: z.boolean(),
  serverFilter: z.array(z.object({ uuid: z.string(), name: z.string() })),
  headers: z.array(headerSchema),
  bodyTemplate: z.string().nullable(),
  secret: z.string().nullable(),
  redacted: z.boolean(),
  lastDelivery: lastDeliverySchema.nullable(),
  created: z.coerce.date(),
  updated: z.coerce.date(),
});
export type Webhook = z.infer<typeof webhookSchema>;

export const webhookPayloadSchema = z.object({
  name: z.string().min(1).max(255),
  url: z.string().max(2048),
  format: formatSchema,
  events: z.array(z.string()).min(1).max(MAX_EVENTS),
  enabled: z.boolean(),
  serverFilter: z.array(z.string()).max(MAX_SERVER_FILTER).optional(),
  headers: z.array(headerSchema).max(MAX_HEADERS),
  bodyTemplate: z.string().max(MAX_BODY_TEMPLATE).nullable(),
});
export type WebhookPayload = z.infer<typeof webhookPayloadSchema>;

export const webhookPatchSchema = webhookPayloadSchema.partial();
export type WebhookPatch = z.infer<typeof webhookPatchSchema>;

export const deliverySummarySchema = z.object({
  uuid: z.string(),
  webhookUuid: z.string(),
  webhookName: z.string(),
  event: z.string(),
  status: deliveryStatusSchema,
  attempts: z.number().int(),
  responseStatus: z.number().int().nullable(),
  error: z.string().nullable(),
  durationMs: z.number().int().nullable(),
  nextAttempt: z.coerce.date().nullable(),
  created: z.coerce.date(),
  completed: z.coerce.date().nullable(),
});
export type DeliverySummary = z.infer<typeof deliverySummarySchema>;

export const deliverySchema = deliverySummarySchema.extend({
  // opaque event envelope: keys stay snake_case exactly as sent
  payload: z.json(),
  requestBody: z.string().nullable(),
  responseBody: z.string().nullable(),
});
export type Delivery = z.infer<typeof deliverySchema>;

export const settingsSchema = z.object({
  maxAttempts: z.number().int().min(1).max(20),
  requestTimeoutSeconds: z.number().int().min(1).max(60),
  retentionDays: z.number().int().min(1).max(365),
  statePollIntervalSeconds: z.number().int().min(0).max(3600),
  serverWebhooksEnabled: z.boolean(),
  serverWebhookLimit: z.number().int().min(0).max(100),
  accountWebhooksEnabled: z.boolean(),
  accountWebhookLimit: z.number().int().min(0).max(100),
  allowPrivateTargets: z.boolean(),
});
export type WebhookSettings = z.infer<typeof settingsSchema>;

export const previewRequestSchema = z.object({
  format: formatSchema,
  bodyTemplate: z.string().nullable(),
  event: z.string(),
});
export type PreviewRequest = z.infer<typeof previewRequestSchema>;

export const eventCatalogSchema = z.object({
  events: z.array(eventInfoSchema),
  defaultTemplates: defaultTemplatesSchema,
});
export type EventCatalog = z.infer<typeof eventCatalogSchema>;

// list response of the server and the account API
export const clientWebhooksSchema = z.object({
  webhooks: z.array(webhookSchema),
  limit: z.number().int(),
  enabled: z.boolean(),
  events: z.array(eventInfoSchema),
  defaultTemplates: defaultTemplatesSchema,
});
export type ClientWebhooks = z.infer<typeof clientWebhooksSchema>;

export type WebhookServerRef = { uuid: string; name: string };

export type WebhookScopeFilter = 'all' | 'global' | 'server' | 'account';

export interface DeliveryFilters {
  webhook?: string | null;
  status?: DeliveryStatus | null;
  event?: string | null;
}

// admin

export const getAdminSettings = async (): Promise<WebhookSettings> => {
  const { data } = await axiosInstance.get(`${WEBHOOKS_ADMIN_BASE}/settings`);
  return parseFromApi(settingsSchema, data.settings);
};

export const updateAdminSettings = async (settings: WebhookSettings): Promise<void> => {
  await axiosInstance.put(`${WEBHOOKS_ADMIN_BASE}/settings`, serializeForApi(settingsSchema, settings));
};

export const getAdminEvents = async (): Promise<EventCatalog> => {
  const { data } = await axiosInstance.get(`${WEBHOOKS_ADMIN_BASE}/events`);
  return parseFromApi(eventCatalogSchema, data);
};

export const getAdminWebhooks = async (
  page: number,
  search: string,
  scope: WebhookScopeFilter = 'all',
): Promise<Pagination<Webhook>> => {
  const { data } = await axiosInstance.get(`${WEBHOOKS_ADMIN_BASE}/webhooks`, {
    params: { page, search: search || undefined, scope },
  });
  return parsePaginationFromApi(webhookSchema, data.webhooks);
};

export const createAdminWebhook = async (payload: WebhookPayload): Promise<Webhook> => {
  const { data } = await axiosInstance.post(
    `${WEBHOOKS_ADMIN_BASE}/webhooks`,
    serializeForApi(webhookPayloadSchema, payload),
  );
  return parseFromApi(webhookSchema, data.webhook);
};

export const getAdminWebhook = async (webhookUuid: string): Promise<Webhook> => {
  const { data } = await axiosInstance.get(`${WEBHOOKS_ADMIN_BASE}/webhooks/${webhookUuid}`);
  return parseFromApi(webhookSchema, data.webhook);
};

export const updateAdminWebhook = async (webhookUuid: string, payload: WebhookPatch): Promise<Webhook> => {
  const { data } = await axiosInstance.patch(
    `${WEBHOOKS_ADMIN_BASE}/webhooks/${webhookUuid}`,
    serializeForApi(webhookPatchSchema, payload),
  );
  return parseFromApi(webhookSchema, data.webhook);
};

export const deleteAdminWebhook = async (webhookUuid: string): Promise<void> => {
  await axiosInstance.delete(`${WEBHOOKS_ADMIN_BASE}/webhooks/${webhookUuid}`);
};

export const testAdminWebhook = async (webhookUuid: string): Promise<Delivery> => {
  const { data } = await axiosInstance.post(`${WEBHOOKS_ADMIN_BASE}/webhooks/${webhookUuid}/test`);
  return parseFromApi(deliverySchema, data.delivery);
};

export const rotateAdminWebhookSecret = async (webhookUuid: string): Promise<string> => {
  const { data } = await axiosInstance.post(`${WEBHOOKS_ADMIN_BASE}/webhooks/${webhookUuid}/rotate-secret`);
  return z.string().parse(data.secret);
};

export const getAdminDeliveries = async (
  page: number,
  filters: DeliveryFilters = {},
): Promise<Pagination<DeliverySummary>> => {
  const { data } = await axiosInstance.get(`${WEBHOOKS_ADMIN_BASE}/deliveries`, {
    params: {
      page,
      webhook: filters.webhook || undefined,
      status: filters.status || undefined,
      event: filters.event || undefined,
    },
  });
  return parsePaginationFromApi(deliverySummarySchema, data.deliveries);
};

export const getAdminDelivery = async (deliveryUuid: string): Promise<Delivery> => {
  const { data } = await axiosInstance.get(`${WEBHOOKS_ADMIN_BASE}/deliveries/${deliveryUuid}`);
  return parseFromApi(deliverySchema, data.delivery);
};

export const redeliverAdminDelivery = async (deliveryUuid: string): Promise<DeliverySummary> => {
  const { data } = await axiosInstance.post(`${WEBHOOKS_ADMIN_BASE}/deliveries/${deliveryUuid}/redeliver`);
  return parseFromApi(deliverySummarySchema, data.delivery);
};

export const previewAdminTemplate = async (request: PreviewRequest): Promise<string> => {
  const { data } = await axiosInstance.post(
    `${WEBHOOKS_ADMIN_BASE}/preview`,
    serializeForApi(previewRequestSchema, request),
  );
  return z.string().parse(data.body);
};

// scope adapters: the shared components talk to the admin, server or account API through these

type DeliveryRef = Pick<DeliverySummary, 'uuid' | 'webhookUuid'>;

export interface WebhookScopeApi {
  queryKey: readonly unknown[];
  createWebhook: (payload: WebhookPayload) => Promise<Webhook>;
  updateWebhook: (webhookUuid: string, payload: WebhookPatch) => Promise<Webhook>;
  deleteWebhook: (webhookUuid: string) => Promise<void>;
  testWebhook: (webhookUuid: string) => Promise<Delivery>;
  rotateSecret: (webhookUuid: string) => Promise<string>;
  preview: (request: PreviewRequest) => Promise<string>;
  listDeliveries: (page: number, filters: DeliveryFilters) => Promise<Pagination<DeliverySummary>>;
  getDelivery: (delivery: DeliveryRef) => Promise<Delivery>;
  redeliver: (delivery: DeliveryRef) => Promise<DeliverySummary>;
  // servers selectable for the server filter; scopes without one never send a filter
  filterServers?: (search: string) => Promise<Pagination<WebhookServerRef>>;
}

export const adminWebhookApi: WebhookScopeApi = {
  queryKey: ['dev.caloptreyx.webhooks', 'admin'],
  createWebhook: createAdminWebhook,
  updateWebhook: updateAdminWebhook,
  deleteWebhook: deleteAdminWebhook,
  testWebhook: testAdminWebhook,
  rotateSecret: rotateAdminWebhookSecret,
  preview: previewAdminTemplate,
  listDeliveries: getAdminDeliveries,
  getDelivery: (delivery) => getAdminDelivery(delivery.uuid),
  redeliver: (delivery) => redeliverAdminDelivery(delivery.uuid),
  filterServers: (search) => getAdminServers(1, search),
};

// server and account webhooks share the client API shape, only the base path differs
export interface ClientWebhookScopeApi extends WebhookScopeApi {
  listWebhooks: () => Promise<ClientWebhooks>;
}

const clientWebhookApi = (
  base: string,
  queryKey: readonly unknown[],
  filterServers?: WebhookScopeApi['filterServers'],
): ClientWebhookScopeApi => {
  const body = <T extends WebhookPatch>(payload: T): T =>
    filterServers ? payload : { ...payload, serverFilter: undefined };

  return {
    queryKey,
    filterServers,
    listWebhooks: async () => {
      const { data } = await axiosInstance.get(base);
      return parseFromApi(clientWebhooksSchema, data);
    },
    createWebhook: async (payload) => {
      const { data } = await axiosInstance.post(base, serializeForApi(webhookPayloadSchema, body(payload)));
      return parseFromApi(webhookSchema, data.webhook);
    },
    updateWebhook: async (webhookUuid, payload) => {
      const { data } = await axiosInstance.patch(
        `${base}/${webhookUuid}`,
        serializeForApi(webhookPatchSchema, body(payload)),
      );
      return parseFromApi(webhookSchema, data.webhook);
    },
    deleteWebhook: async (webhookUuid) => {
      await axiosInstance.delete(`${base}/${webhookUuid}`);
    },
    testWebhook: async (webhookUuid) => {
      const { data } = await axiosInstance.post(`${base}/${webhookUuid}/test`);
      return parseFromApi(deliverySchema, data.delivery);
    },
    rotateSecret: async (webhookUuid) => {
      const { data } = await axiosInstance.post(`${base}/${webhookUuid}/rotate-secret`);
      return z.string().parse(data.secret);
    },
    preview: async (request) => {
      const { data } = await axiosInstance.post(`${base}/preview`, serializeForApi(previewRequestSchema, request));
      return z.string().parse(data.body);
    },
    listDeliveries: async (page, filters) => {
      // the client API only lists deliveries per webhook
      if (!filters.webhook) {
        throw new Error('A webhook is required to list its deliveries.');
      }
      const { data } = await axiosInstance.get(`${base}/${filters.webhook}/deliveries`, { params: { page } });
      return parsePaginationFromApi(deliverySummarySchema, data.deliveries);
    },
    getDelivery: async (delivery) => {
      const { data } = await axiosInstance.get(`${base}/${delivery.webhookUuid}/deliveries/${delivery.uuid}`);
      return parseFromApi(deliverySchema, data.delivery);
    },
    redeliver: async (delivery) => {
      const { data } = await axiosInstance.post(
        `${base}/${delivery.webhookUuid}/deliveries/${delivery.uuid}/redeliver`,
      );
      return parseFromApi(deliverySummarySchema, data.delivery);
    },
  };
};

export const serverWebhookApi = (serverUuid: string): ClientWebhookScopeApi =>
  clientWebhookApi(webhooksServerBase(serverUuid), ['dev.caloptreyx.webhooks', 'server', serverUuid]);

// account webhooks may cover servers the user owns or holds `webhooks.read` on as a subuser
const getAccountFilterServers = async (search: string): Promise<Pagination<WebhookServerRef>> => {
  const servers = await getServers(1, search);
  return {
    ...servers,
    data: servers.data.filter((server) => server.isOwner || server.permissions.includes('webhooks.read')),
  };
};

export const accountWebhookApi: ClientWebhookScopeApi = clientWebhookApi(
  WEBHOOKS_ACCOUNT_BASE,
  ['dev.caloptreyx.webhooks', 'account'],
  getAccountFilterServers,
);
