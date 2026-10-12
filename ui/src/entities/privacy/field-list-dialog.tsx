import { Code, List, Modal, ScrollArea, Stack, Text, Title } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { type FieldList, type ListedEvent, type ListedField, loadFieldList } from '../../shared/telemetry/field-list';
import { noteViewed } from './queries';

const TYPES = {
	number: 'desktop.privacy.typeNumber',
	flag: 'desktop.privacy.typeFlag',
	id: 'desktop.privacy.typeId',
	version: 'desktop.privacy.typeVersion',
} as const;

function Field({ field }: { field: ListedField }) {
	const { t } = useTranslation();
	const type =
		field.type === 'choice'
			? t('desktop.privacy.typeChoice', { values: (field.values ?? []).join(', ') })
			: t(TYPES[field.type]);
	return (
		<List.Item>
			<Code>{field.name}</Code>
			{`: ${type}`}
		</List.Item>
	);
}

function EventEntry({ event }: { event: ListedEvent }) {
	return (
		<Stack gap={2}>
			<Code fw={600}>{event.name}</Code>
			<Text size="sm">{event.purpose}</Text>
			<List listStyleType="none" size="xs" spacing={2}>
				{event.fields.map((field) => (
					<Field field={field} key={field.name} />
				))}
			</List>
		</Stack>
	);
}

function Section({ title, events }: { title: string; events: ListedEvent[] }) {
	return (
		<Stack gap="sm">
			<Title order={5}>{title}</Title>
			{events.map((event) => (
				<EventEntry event={event} key={event.name} />
			))}
		</Stack>
	);
}

function Body({ list }: { list: FieldList }) {
	const { t } = useTranslation();
	const category = (name: ListedEvent['category']) => list.events.filter((event) => event.category === name);
	return (
		<Stack gap="md">
			<Text size="sm">{t('desktop.privacy.intro')}</Text>
			<Text c="stone.7" size="xs">
				{t('desktop.privacy.retention', { days: list.retentionDays })}
			</Text>
			<ScrollArea.Autosize
				mah="min(55vh, calc(100vh - 260px))"
				type="auto"
				viewportProps={{ tabIndex: 0, 'aria-label': t('desktop.privacy.whatIsSent') }}
			>
				<Stack gap="lg" pr="sm">
					<Section events={category('diagnostics')} title={t('desktop.privacy.diagnostics')} />
					<Section events={category('usage')} title={t('desktop.privacy.usage')} />
				</Stack>
			</ScrollArea.Autosize>
		</Stack>
	);
}

export function FieldListDialog({ onClose }: { onClose: () => void }) {
	const { t, i18n } = useTranslation();
	const list = useQuery({
		queryKey: ['fieldList', i18n.language],
		queryFn: () => loadFieldList(i18n.language),
		staleTime: Number.POSITIVE_INFINITY,
	}).data;
	useEffect(() => noteViewed('whatIsSent'), []);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.privacy.close') }}
			onClose={onClose}
			opened
			size="xl"
			title={t('desktop.privacy.whatIsSent')}
		>
			{list && <Body list={list} />}
		</Modal>
	);
}
