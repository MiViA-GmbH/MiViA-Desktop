import { Paper, Stack, Text, Title } from '@mantine/core';
import type { ReactNode } from 'react';

type EmptyStateProps = { title: string; text: ReactNode; action?: ReactNode; hint?: ReactNode };

export function EmptyState({ title, text, action, hint }: EmptyStateProps) {
	return (
		<Paper p="xl" withBorder>
			<Stack align="center" gap="sm">
				<Title order={3} ta="center">
					{title}
				</Title>
				<Text c="stone.7" maw={440} size="sm" ta="center">
					{text}
				</Text>
				{action}
				{hint && (
					<Text c="stone.7" maw={440} size="xs" ta="center">
						{hint}
					</Text>
				)}
			</Stack>
		</Paper>
	);
}
