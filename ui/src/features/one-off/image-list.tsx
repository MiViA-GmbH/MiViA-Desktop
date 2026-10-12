import { Button, Group, Paper, Stack, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { ImageGroupView, NotIncludedReason, NotIncludedView, OneOffView } from '../../shared/ipc/bindings';
import { PathText } from '../../shared/ui/path-text';

const SHOWN = 3;

const REASONS = {
	missing: 'desktop.oneOff.reason.missing',
	notImage: 'desktop.oneOff.reason.notImage',
	zipTemp: 'desktop.oneOff.reason.zipTemp',
} as const satisfies Record<NotIncludedReason, string>;

type GroupProps = { group: ImageGroupView; index: number; removable: boolean };

function ImageGroup({ group, index, removable }: GroupProps) {
	const { t } = useTranslation();
	const [all, setAll] = useState(false);
	const remove = useMutation({ mutationFn: () => commands.discardOneOff(index) });
	const hidden = group.names.length - SHOWN;
	const names = all ? group.names : group.names.slice(0, SHOWN);
	return (
		<Stack gap={2}>
			{names.map((name) => (
				<Text key={name} size="sm" truncate>
					{name}
				</Text>
			))}
			{hidden > 0 && !all && (
				<Button onClick={() => setAll(true)} p={0} size="xs" variant="transparent" w="fit-content">
					{t('desktop.oneOff.more', { count: hidden })}
				</Button>
			)}
			<Group gap="xs" justify="space-between" wrap="nowrap">
				<PathText c="stone.7" size="xs">
					{t('desktop.oneOff.from', { folder: group.folder })}
				</PathText>
				{removable && (
					<Button loading={remove.isPending} onClick={() => remove.mutate()} size="xs" variant="subtle">
						{t('desktop.oneOff.remove')}
					</Button>
				)}
			</Group>
		</Stack>
	);
}

function Skipped({ item }: { item: NotIncludedView }) {
	const { t } = useTranslation();
	return (
		<Text c="stone.7" size="xs">
			{t('desktop.oneOff.notIncluded', { name: item.name, reason: t(REASONS[item.reason]) })}
		</Text>
	);
}

export function NotIncludedList({ view }: { view: OneOffView }) {
	const { t } = useTranslation();
	return (
		<>
			{view.notIncluded.map((item) => (
				<Skipped item={item} key={item.name} />
			))}
			{view.subfolders > 0 && (
				<Text c="stone.7" size="xs">
					{t('desktop.oneOff.subfolders', { count: view.subfolders })}
				</Text>
			)}
		</>
	);
}

export function ImageList({ view }: { view: OneOffView }) {
	return (
		<Paper p="sm" withBorder>
			<Stack gap="sm">
				{view.groups.map((group, index) => (
					<ImageGroup group={group} index={index} key={group.folder} removable={view.groups.length > 1} />
				))}
				<NotIncludedList view={view} />
			</Stack>
		</Paper>
	);
}
