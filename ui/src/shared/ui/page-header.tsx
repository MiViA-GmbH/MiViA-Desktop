import { Group, Stack, Text, Title } from '@mantine/core';
import type { ReactNode } from 'react';

type PageHeaderProps = { title: ReactNode; meta?: ReactNode; actions?: ReactNode; titleExtra?: ReactNode };

export function PageHeader({ title, meta, actions, titleExtra }: PageHeaderProps) {
	return (
		<Group align="flex-end" gap="md" justify="space-between" wrap="nowrap">
			<Stack gap={4} miw={0}>
				<Group gap="xs" wrap="nowrap">
					<Title order={2}>{title}</Title>
					{titleExtra}
				</Group>
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
