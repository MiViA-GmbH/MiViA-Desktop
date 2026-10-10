import { Alert } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import type { ReactNode } from 'react';

import { ErrorIcon, InfoIcon, SuccessIcon, WarningIcon } from './icons';

const LOOK = {
	warn: { color: 'orange', icon: WarningIcon },
	error: { color: 'red', icon: ErrorIcon },
	success: { color: 'forest', icon: SuccessIcon },
	info: { color: 'jade', icon: InfoIcon },
} as const;

type NoticeProps = { tone: keyof typeof LOOK; title?: ReactNode; children?: ReactNode };

export function Notice({ tone, title, children }: NoticeProps) {
	const { color, icon: NoticeIcon } = LOOK[tone];
	return (
		<Alert color={color} icon={<NoticeIcon size={20} stroke={ICON_STROKE} />} title={title}>
			{children}
		</Alert>
	);
}
