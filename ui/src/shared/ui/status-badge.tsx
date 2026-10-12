import { Badge } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';

import { type Status, TONE_COLOR } from './tone';

export function StatusBadge({ status }: { status: Status }) {
	const StatusIcon = status.icon;
	return (
		<Badge color={TONE_COLOR[status.tone]} flex="none" leftSection={<StatusIcon size={14} stroke={ICON_STROKE} />}>
			{status.label}
		</Badge>
	);
}
