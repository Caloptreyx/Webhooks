import { useQuery } from '@tanstack/react-query';
import { getAdminEvents } from '../../api.ts';

// the event catalog only changes with extension updates
export default function useAdminEvents() {
  return useQuery({
    queryKey: ['dev.caloptreyx.webhooks', 'admin', 'events'],
    queryFn: getAdminEvents,
    staleTime: Infinity,
  });
}
