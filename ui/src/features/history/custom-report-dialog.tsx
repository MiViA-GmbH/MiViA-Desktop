import '../../shared/ui/forms.css';

import { Button, Checkbox, Group, Loader, Modal, Radio, Select, Stack, Text, TextInput } from '@mantine/core';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { customDraftQuery } from '../../entities/series/queries';
import { notifyStatus } from '../../entities/support/toasts';
import { commands } from '../../shared/ipc';
import type { CustomReportDraft, FolderPick, Language, ReportOrder } from '../../shared/ipc/bindings';
import { FormatChoice } from '../../shared/ui/format-choice';
import { Notice } from '../../shared/ui/notice';
import { PathText } from '../../shared/ui/path-text';
import { DraftSummary } from './draft-summary';

type Form = {
	name: string;
	pdf: boolean;
	csv: boolean;
	includeImages: boolean;
	language: Language;
	decimalComma: boolean;
	order: ReportOrder;
	folder: FolderPick | null;
};

type DialogProps = { series: string[]; csvOnly?: boolean; onClose: () => void };

const LANGUAGES = [
	{ value: 'en', label: 'English' },
	{ value: 'de', label: 'Deutsch' },
];

function isLanguage(value: string | null): value is Language {
	return value === 'en' || value === 'de';
}

function useCreate(series: string[], form: Form, onClose: () => void) {
	const { t } = useTranslation();
	return useMutation({
		mutationFn: () =>
			commands.createCustomReport({
				series,
				name: form.name,
				formats: [form.pdf && 'pdf', form.csv && 'csv'].filter((format) => typeof format === 'string'),
				language: form.language,
				decimalComma: form.decimalComma,
				includeImages: form.includeImages,
				order: form.order,
				folderPick: form.folder?.pickId ?? null,
			}),
		onSuccess: () => {
			const message = t('desktop.custom.created');
			notifyStatus('desktop.toast.reportCreated', message);
			onClose();
		},
	});
}

type FieldsProps = { draft: CustomReportDraft; form: Form; update: (changes: Partial<Form>) => void };

function FormatFields({ form, update }: Omit<FieldsProps, 'draft'>) {
	const { t } = useTranslation();
	return (
		<Stack gap="xs">
			<FormatChoice form={form} update={update} />
			{form.csv && (
				<Group gap="lg">
					<Checkbox
						checked={form.includeImages}
						label={t('desktop.custom.includeImages')}
						onChange={(event) => update({ includeImages: event.currentTarget.checked })}
					/>
					<Checkbox
						checked={form.decimalComma}
						label={t('desktop.custom.decimalComma')}
						onChange={(event) => update({ decimalComma: event.currentTarget.checked })}
					/>
				</Group>
			)}
		</Stack>
	);
}

function SettingsFields({ draft, form, update }: FieldsProps) {
	const { t } = useTranslation();
	const pick = useMutation({
		mutationFn: () => commands.pickFolder('reports'),
		onSuccess: (folder) => folder && update({ folder }),
	});
	return (
		<Stack gap="sm">
			<FormatFields form={form} update={update} />
			<Select
				allowDeselect={false}
				data={LANGUAGES}
				label={t('desktop.oneOff.reportLanguage')}
				maw={240}
				onChange={(value) => isLanguage(value) && update({ language: value, decimalComma: value === 'de' })}
				value={form.language}
			/>
			<Radio.Group
				label={t('desktop.custom.order')}
				onChange={(value) => update({ order: value === 'date' ? 'date' : 'name' })}
				value={form.order}
			>
				<Group gap="lg" mt={4} style={{ rowGap: 8 }}>
					<Radio label={t('desktop.custom.byName')} value="name" />
					<Radio label={t('desktop.custom.byDate')} value="date" />
				</Group>
			</Radio.Group>
			<Stack gap={4}>
				<Text fw={500} size="sm">
					{t('desktop.custom.folder')}
				</Text>
				<PathText size="sm">{form.folder?.folder.display ?? draft.folder}</PathText>
				<Button
					loading={pick.isPending}
					onClick={() => pick.mutate()}
					size="xs"
					style={{ alignSelf: 'flex-start' }}
					variant="default"
				>
					{t('desktop.oneOff.chooseFolder')}
				</Button>
			</Stack>
		</Stack>
	);
}

function Body({ draft, series, csvOnly = false, onClose }: DialogProps & { draft: CustomReportDraft }) {
	const { t } = useTranslation();
	const [form, setForm] = useState<Form>(() => ({
		name: '',
		pdf: !csvOnly,
		csv: csvOnly,
		includeImages: false,
		language: draft.language,
		decimalComma: draft.language === 'de',
		order: 'name',
		folder: null,
	}));
	const [nameError, setNameError] = useState(false);
	const update = (changes: Partial<Form>) => setForm((current) => ({ ...current, ...changes }));
	const create = useCreate(series, form, onClose);
	const submit = () => {
		const missing = form.name.trim().length === 0;
		setNameError(missing);
		if (!missing) {
			create.mutate();
		}
	};
	const ready = (form.pdf || form.csv) && !draft.tooMany;
	return (
		<Stack gap="md">
			{draft.tooMany && <Notice tone="error">{t('desktop.custom.tooMany')}</Notice>}
			<Group align="flex-start" gap="lg" grow wrap="nowrap">
				<Stack gap="md">
					<DraftSummary draft={draft} />
					<TextInput
						data-autofocus
						error={nameError && t('desktop.custom.nameRequired')}
						label={t('desktop.custom.name')}
						maxLength={200}
						onChange={(event) => update({ name: event.currentTarget.value })}
						required
						value={form.name}
					/>
				</Stack>
				<SettingsFields draft={draft} form={form} update={update} />
			</Group>
			<Group gap="sm" justify="flex-end">
				<Button onClick={onClose} variant="default">
					{t('desktop.custom.cancel')}
				</Button>
				<Button disabled={!ready} loading={create.isPending} onClick={submit}>
					{t('desktop.custom.create')}
				</Button>
			</Group>
		</Stack>
	);
}

export function CustomReportDialog({ series, csvOnly = false, onClose }: DialogProps) {
	const { t } = useTranslation();
	const draft = useQuery(customDraftQuery(series)).data;
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{t('desktop.custom.title')}
		</Text>
	);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.custom.cancel') }}
			onClose={onClose}
			opened
			size="xl"
			title={title}
			yOffset="2dvh"
		>
			{draft ? <Body csvOnly={csvOnly} draft={draft} onClose={onClose} series={series} /> : <Loader size="sm" />}
		</Modal>
	);
}
