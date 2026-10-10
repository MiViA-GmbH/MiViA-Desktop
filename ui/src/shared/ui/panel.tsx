import { Divider, Group, Paper, Title } from '@mantine/core';
import type { ReactNode } from 'react';

type PanelProps = { title?: ReactNode; extra?: ReactNode; children: ReactNode };

export function Panel({ title, extra, children }: PanelProps) {
	return (
		<Paper p="md" withBorder>
			{title && (
				<>
					<Group gap="sm" justify="space-between" pb="sm" wrap="nowrap">
						<Title c="stone.7" fw={600} order={4}>
							{title}
						</Title>
						{extra}
					</Group>
					<Divider color="stone.3" mb="sm" />
				</>
			)}
			{children}
		</Paper>
	);
}
