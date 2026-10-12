import { Box, Button, Group, Select, Stack, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { FolderPick, Language, OneOffView } from '../../shared/ipc/bindings';
import { FormatChoice } from '../../shared/ui/format-choice';
import { Notice } from '../../shared/ui/notice';
import { PathText } from '../../shared/ui/path-text';

export type ReportForm = { pdf: boolean; csv: boolean; language: Language; reports: FolderPick | null; open: boolean };

const LANGUAGES = [
	{ value: 'en', label: 'English' },
	{ value: 'de', label: 'Deutsch' },
];

function isLanguage(value: string | null): value is Language {
	return value === 'en' || value === 'de';
}

type ReportProps = { view: OneOffView; form: ReportForm; update: (changes: Partial<ReportForm>) => void };

function folderOf(view: OneOffView, form: ReportForm): string {
	return form.reports?.folder.display ?? view.reportFolder;
}

function ReportFolder({ view, form, update }: ReportProps) {
	const { t } = useTranslation();
	const pick = useMutation({
		mutationFn: () => commands.pickFolder('reports'),
		onSuccess: (reports) => reports && update({ reports }),
	});
	return (
		<Stack gap={4}>
			<Text fw={500} size="sm">
				{t('desktop.oneOff.reportFolder')}
			</Text>
			<Group gap="sm" wrap="nowrap">
				<PathText size="sm">{folderOf(view, form)}</PathText>
				<Button flex="none" loading={pick.isPending} onClick={() => pick.mutate()} size="xs" variant="default">
					{t('desktop.oneOff.chooseFolder')}
				</Button>
			</Group>
		</Stack>
	);
}

function ReportSettingsFields({ view, form, update }: ReportProps) {
	const { t } = useTranslation();
	return (
		<Stack gap="sm">
			<Group align="flex-end" gap="md" wrap="nowrap">
				<Select
					allowDeselect={false}
					data={LANGUAGES}
					label={t('desktop.settings.language')}
					onChange={(value) => isLanguage(value) && update({ language: value })}
					value={form.language}
					w={110}
				/>
				<Box pb={8}>
					<FormatChoice form={form} update={update} />
				</Box>
			</Group>
			<ReportFolder form={form} update={update} view={view} />
		</Stack>
	);
}

export function ReportChoice({ view, form, update }: ReportProps) {
	const { t } = useTranslation();
	const formats = [form.pdf && 'PDF', form.csv && t('desktop.oneOff.formatCsv')].filter(Boolean).join(' + ');
	const language = LANGUAGES.find((entry) => entry.value === form.language)?.label ?? '';
	return (
		<Stack gap="xs">
			{!form.open && (
				<PathText c="stone.7" size="sm">
					{t('desktop.oneOff.report', { formats, language, folder: folderOf(view, form) })}
				</PathText>
			)}
			{!form.open && (
				<Button onClick={() => update({ open: true })} px={0} size="xs" variant="transparent" w="fit-content">
					{t('desktop.oneOff.changeReport')}
				</Button>
			)}
			{view.reportFallback && !form.reports && (
				<Notice tone="warn">{t('desktop.oneOff.fallback', { path: view.reportFolder })}</Notice>
			)}
			{form.open && <ReportSettingsFields form={form} update={update} view={view} />}
		</Stack>
	);
}
