import { Tooltip } from '@mantine/core';
import type { ReactElement } from 'react';

export function RailTooltip({ label, children }: { label: string; children: ReactElement }) {
	return (
		<Tooltip label={label} position="right">
			{children}
		</Tooltip>
	);
}
