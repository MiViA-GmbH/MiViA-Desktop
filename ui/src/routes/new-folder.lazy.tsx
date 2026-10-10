import '../shared/ui/forms.css';

import { Button, Group, Loader, Stack, Text } from '@mantine/core';
import { useMutation, useQuery } from '@tanstack/react-query';
import { createLazyRoute, useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ModelChoices } from '../entities/catalogue/model-choices';
import { catalogueQuery, setupQuery } from '../entities/setup/queries';
import { PresetChoice, ReportChoice } from '../features/setup/arrival-choice';
import { FolderChoice, FolderDetails } from '../features/setup/folder-choice';
import { blockText } from '../features/setup/folder-text';
import { commands } from '../shared/ipc';
import type { ArrivalPreset, FolderPick, ModelChoiceView, ReportMode } from '../shared/ipc/bindings';
import { Notice } from '../shared/ui/notice';
import { PageHeader } from '../shared/ui/page-header';
import { Panel } from '../shared/ui/panel';

type Form = {
	folder: FolderPick | null;
	reports: FolderPick | null;
	preset: ArrivalPreset;
	mode: ReportMode;
	models: ModelChoiceView[];
	include: boolean;
};

const EMPTY: Form = { folder: null, reports: null, preset: 'byHand', mode: 'subfolder', models: [], include: false };

function useAdd(form: Form) {
	const navigate = useNavigate();
	return useMutation({
		mutationFn: (pick: string) =>
			commands.addFolder({
				folderPick: pick,
				preset: form.preset,
				reportMode: form.mode,
				reportPick: form.reports?.pickId ?? null,
				models: form.models,
				includeExisting: form.include,
			}),
		onSuccess: (id) => navigate({ to: '/folders/$folderId', params: { folderId: id } }),
	});
}

function ReportsPanel({ form, update }: { form: Form; update: (changes: Partial<Form>) => void }) {
	const pick = useMutation({
		mutationFn: () => commands.pickFolder('reports'),
		onSuccess: (reports) => reports && update({ reports, mode: 'custom' }),
	});
	const base = form.folder?.report;
	if (!base || !form.folder) {
		return null;
	}
	const display = {
		subfolder: base.display,
		beside: form.folder.folder.display,
		custom: form.reports?.folder.display ?? base.display,
	};
	return (
		<ReportChoice
			legacy={false}
			onMode={(mode) => update({ mode })}
			onPickCustom={() => pick.mutate()}
			report={{ ...base, mode: form.mode, display: display[form.mode] }}
		/>
	);
}

function NewFolder() {
	const { t } = useTranslation();
	const setup = useQuery(setupQuery).data;
	const catalogue = useQuery(catalogueQuery).data;
	const [form, setForm] = useState<Form>(EMPTY);
	const update = (changes: Partial<Form>) => setForm({ ...form, ...changes });
	const pick = useMutation({
		mutationFn: () => commands.pickFolder('input'),
		onSuccess: (folder) => folder && update({ folder, reports: null, mode: 'subfolder' }),
	});
	const add = useAdd(form);
	const chosen = form.folder?.folder ?? null;
	const blocked = chosen?.block ? blockText(chosen.block, t) : null;
	return (
		<>
			<PageHeader title={t('desktop.addFolder.page.title')} />
			<Panel title={t('desktop.addFolder.page.folder')}>
				<Stack gap="md">
					<Text c="stone.8" size="sm">
						{t('desktop.wizard.folder.lead')}
					</Text>
					<FolderChoice busy={pick.isPending} error={blocked} folder={chosen} onChoose={() => pick.mutate()} />
					<FolderDetails
						folder={chosen}
						include={form.include}
						onInclude={(include) => update({ include })}
						remaining={setup?.connection?.remainingUploads ?? null}
					/>
				</Stack>
			</Panel>
			<Panel title={t('desktop.addFolder.page.arrival')}>
				<Stack gap="md">
					<Text c="stone.8" size="sm">
						{t('desktop.wizard.series.title')}
					</Text>
					<PresetChoice onChange={(preset) => update({ preset })} preset={form.preset} />
					<ReportsPanel form={form} update={update} />
				</Stack>
			</Panel>
			<Panel title={t('desktop.addFolder.page.models')}>
				{catalogue ? (
					<ModelChoices
						catalogue={catalogue}
						chosen={form.models}
						contact={setup?.supportContact ?? ''}
						error={null}
						host={setup?.host ?? ''}
						onChange={(models) => update({ models })}
					/>
				) : (
					<Loader aria-label={t('desktop.wizard.models.loading')} size="sm" />
				)}
				{catalogue && !catalogue.loaded && <Notice tone="warn">{t('desktop.wizard.models.loadFailed')}</Notice>}
			</Panel>
			<Group justify="flex-end">
				<Button
					disabled={!form.folder || Boolean(blocked)}
					loading={add.isPending}
					onClick={() => form.folder && add.mutate(form.folder.pickId)}
				>
					{t('desktop.addFolder.page.add')}
				</Button>
			</Group>
		</>
	);
}

export const NewFolderRoute = createLazyRoute('/app/folders/new')({ component: NewFolder });
