import { queryOptions } from '@tanstack/react-query';

import { commands } from '../../shared/ipc';
import type { PrivacyPanel } from '../../shared/ipc/bindings';

export const PRIVACY = ['privacy'] as const;

export const privacyQuery = queryOptions({ queryKey: PRIVACY, queryFn: () => commands.getPrivacy() });

export const sentQuery = queryOptions({
	queryKey: [...PRIVACY, 'sent'],
	queryFn: () => commands.listSentTelemetry(),
	staleTime: 0,
});

export function noteViewed(panel: PrivacyPanel): void {
	commands.notePrivacyViewed(panel).catch(() => undefined);
}
