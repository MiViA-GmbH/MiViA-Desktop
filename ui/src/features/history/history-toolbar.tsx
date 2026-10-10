import {
	ActionIcon,
	CloseButton,
	Group,
	Popover,
	SegmentedControl,
	Select,
	Stack,
	Text,
	TextInput,
} from '@mantine/core';
import { useDebouncedCallback } from '@mantine/hooks';
import { ICON_STROKE } from '@mivia-ai/theme';
import { IconAdjustmentsHorizontal, IconSearch } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { foldersQuery } from '../../entities/folder/queries';
import type { HistorySearch, StatusParam } from '../../entities/series/search';
import { catalogueQuery } from '../../entities/setup/queries';

const SEARCH_DELAY = 250;
const MIN_CHARS = 3;
const MANUAL = 'manual';

type ToolbarProps = { search: HistorySearch; onChange: (next: HistorySearch) => void };

function SearchInput({ search, onChange }: ToolbarProps) {
	const { t } = useTranslation();
	const [text, setText] = useState(search.q ?? '');
	const commit = useDebouncedCallback((value: string) => {
		const trimmed = value.trim();
		onChange({ ...search, q: trimmed.length >= MIN_CHARS ? trimmed : undefined });
	}, SEARCH_DELAY);
	const change = (value: string) => {
		setText(value);
		commit(value);
	};
	const short = text.trim().length > 0 && text.trim().length < MIN_CHARS;
	return (
		<TextInput
			aria-label={t('desktop.history.search')}
			description={short ? t('desktop.history.searchHint') : undefined}
			flex={1}
			leftSection={<IconSearch size={16} stroke={ICON_STROKE} />}
			miw={220}
			onChange={(event) => change(event.currentTarget.value)}
			placeholder={t('desktop.history.search')}
			rightSection={text && <CloseButton aria-label={t('desktop.history.clearSearch')} onClick={() => change('')} />}
			value={text}
		/>
	);
}

function FolderSelect({ search, onChange }: ToolbarProps) {
	const { t } = useTranslation();
	const folders = useQuery(foldersQuery).data?.folders ?? [];
	const data = [
		{ value: '', label: t('desktop.history.allFolders') },
		...folders.map((folder) => ({ value: folder.id, label: folder.name })),
		{ value: MANUAL, label: t('desktop.nav.singleImages') },
	];
	const value = search.origin === 'manual' ? MANUAL : (search.folder ?? '');
	const select = (next: string | null) => {
		const manual = next === MANUAL;
		onChange({ ...search, folder: manual || !next ? undefined : next, origin: manual ? 'manual' : undefined });
	};
	return (
		<Select
			allowDeselect={false}
			aria-label={t('desktop.history.folderFilter')}
			data={data}
			onChange={select}
			value={value}
			w={200}
		/>
	);
}

function AnalysisSelect({ search, onChange }: ToolbarProps) {
	const { t } = useTranslation();
	const models = useQuery(catalogueQuery).data?.models ?? [];
	const data = [
		{ value: '', label: t('desktop.history.allAnalyses') },
		...models.map((model) => ({ value: model.modelId, label: model.name })),
	];
	return (
		<Select
			allowDeselect={false}
			comboboxProps={{ withinPortal: false }}
			data={data}
			label={t('desktop.history.analysisFilter')}
			onChange={(next) => onChange({ ...search, analysis: next || undefined })}
			value={search.analysis ?? ''}
		/>
	);
}

function MoreFilters({ search, onChange }: ToolbarProps) {
	const { t } = useTranslation();
	const active = Boolean(search.from || search.to || search.analysis);
	return (
		<Popover position="bottom-end" trapFocus withinPortal>
			<Popover.Target>
				<ActionIcon aria-label={t('desktop.history.moreFilters')} size={36} variant={active ? 'light' : 'default'}>
					<IconAdjustmentsHorizontal size={18} stroke={ICON_STROKE} />
				</ActionIcon>
			</Popover.Target>
			<Popover.Dropdown>
				<Stack gap="xs" w={240}>
					<AnalysisSelect onChange={onChange} search={search} />
					<Text fw={500} pt={4} size="sm">
						{t('desktop.history.dates')}
					</Text>
					<TextInput
						label={t('desktop.history.from')}
						onChange={(event) => onChange({ ...search, from: event.currentTarget.value || undefined })}
						type="date"
						value={search.from ?? ''}
					/>
					<TextInput
						label={t('desktop.history.to')}
						onChange={(event) => onChange({ ...search, to: event.currentTarget.value || undefined })}
						type="date"
						value={search.to ?? ''}
					/>
				</Stack>
			</Popover.Dropdown>
		</Popover>
	);
}

function StatusFilter({ search, onChange }: ToolbarProps) {
	const { t } = useTranslation();
	const data = [
		{ value: 'all', label: t('desktop.history.statusAll') },
		{ value: 'problems', label: t('desktop.history.statusProblems') },
		{ value: 'out-of-date', label: t('desktop.history.statusStale') },
	];
	const pick = (value: string) => {
		const status = value === 'all' ? undefined : (value as StatusParam);
		onChange({ ...search, status });
	};
	return <SegmentedControl data={data} onChange={pick} value={search.status ?? 'all'} />;
}

export function HistoryToolbar(props: ToolbarProps & { manualOnly?: boolean }) {
	return (
		<Group gap="sm" wrap="nowrap">
			<SearchInput {...props} />
			{!props.manualOnly && <FolderSelect {...props} />}
			<StatusFilter {...props} />
			<MoreFilters {...props} />
		</Group>
	);
}
