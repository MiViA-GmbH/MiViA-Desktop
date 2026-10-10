import { Group, Stack, Text, Title } from '@mantine/core';
import type { ReactNode } from 'react';

type PageHeaderProps = { title: ReactNode; meta?: ReactNode; actions?: ReactNode };

export function PageHeader({ title, meta, actions }: PageHeaderProps) {
	return (
		<Group align="flex-end" gap="md" justify="space-between" wrap="nowrap">
			<Stack gap={4} miw={0}>
				<Title order={2}>{title}</Title>
				{meta && (
					<Text c="stone.7" component="div" size="sm">
						{meta}
					</Text>
				)}
			</Stack>
			{actions}
		</Group>
	);
}
