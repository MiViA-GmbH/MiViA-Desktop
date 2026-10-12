import '../../shared/ui/forms.css';

import { Button, Group, Loader, Modal, Stack, Text, TextInput, Title } from '@mantine/core';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { accountQuery } from '../../entities/account/queries';
import { ModelChoices } from '../../entities/catalogue/model-choices';
import { holdsQuery } from '../../entities/hold/queries';
import { remainingUploadsQuery } from '../../entities/one-off/queries';
import { catalogueQuery, setupQuery } from '../../entities/setup/queries';
import { notifyStatus } from '../../entities/support/toasts';
import { commands } from '../../shared/ipc';
import type { CatalogueView, ModelChoiceView, OneOffView } from '../../shared/ipc/bindings';
import { formatDateTime } from '../../shared/lib/format';
import { Notice } from '../../shared/ui/notice';
import { ImageList, NotIncludedList } from './image-list';
import { ReportChoice, type ReportForm } from './report-choice';

type Form = ReportForm & { label: string; models: ModelChoiceView[] };

function useCancel() {
	const { t } = useTranslation();
	return useMutation({
		mutationFn: () => commands.discardOneOff(null),
		onSuccess: () => {
			const message = t('desktop.oneOff.cancelled');
			notifyStatus('desktop.toast.cancelled', message);
		},
	});
}

function initialModels(view: OneOffView, catalogue: CatalogueView | undefined): ModelChoiceView[] {
	const known = (choice: ModelChoiceView) => catalogue?.models.some((model) => model.modelId === choice.modelId);
	const last = view.lastChoices.filter(known);
	return last.length > 0 ? last : (catalogue?.preselected ?? []);
}

function useActions(form: Form) {
	const navigate = useNavigate();
	const analyze = useMutation({
		mutationFn: () =>
			commands.confirmOneOff({
				label: form.label,
				models: form.models,
				pdf: form.pdf,
				csv: form.csv,
				language: form.language,
				reportPick: form.reports?.pickId ?? null,
			}),
		onSuccess: (id) => navigate({ to: '/series/$seriesId', params: { seriesId: id } }),
	});
	return { analyze, cancel: useCancel() };
}

function Models({ view, form, update }: { view: OneOffView; form: Form; update: (changes: Partial<Form>) => void }) {
	const { t } = useTranslation();
	const catalogue = useQuery(catalogueQuery);
	const setup = useQuery(setupQuery).data;
	if (!catalogue.data) {
		return <Loader aria-label={t('desktop.wizard.models.loading')} size="sm" />;
	}
	const title = (
		<Title c="stone.7" fw={600} order={4}>
			{t('desktop.oneOff.models')}
		</Title>
	);
	if (form.open) {
		const chosen = new Set(form.models.map((model) => model.modelId));
		const names = catalogue.data.models.filter((model) => chosen.has(model.modelId)).map((model) => model.name);
		return (
			<Stack align="flex-start" gap="xs">
				{title}
				<Text size="sm">{names.join(', ')}</Text>
				<Button onClick={() => update({ open: false })} px={0} size="xs" variant="transparent">
					{t('desktop.oneOff.changeModels')}
				</Button>
			</Stack>
		);
	}
	return (
		<Stack gap="xs">
			{title}
			{!catalogue.data.loaded && (
				<Notice tone="warn" title={t('desktop.wizard.models.loadFailed')}>
					<Button onClick={() => catalogue.refetch()} size="xs" variant="default">
						{t('desktop.oneOff.tryAgain')}
					</Button>
				</Notice>
			)}
			<ModelChoices
				catalogue={catalogue.data}
				compact
				chosen={form.models}
				contact={setup?.supportContact ?? ''}
				error={null}
				host={setup?.host ?? ''}
				onChange={(models) => update({ models })}
			/>
			{view.lastChosenAt && form.models.length > 0 && (
				<Text c="stone.7" size="xs">
					{t('desktop.oneOff.lastTime', { date: formatDateTime(view.lastChosenAt) })}
				</Text>
			)}
		</Stack>
	);
}

function CostLine({ count }: { count: number }) {
	const { t } = useTranslation();
	const remaining = useQuery(remainingUploadsQuery).data;
	if (remaining === undefined || remaining === null) {
		return null;
	}
	return (
		<Text c="stone.8" size="sm">
			{t('desktop.oneOff.uploadCost', { count, remaining })}
		</Text>
	);
}

function Footer({ view, form, actions }: { view: OneOffView; form: Form; actions: ReturnType<typeof useActions> }) {
	const { t } = useTranslation();
	const account = useQuery(accountQuery).data;
	const hasFormat = form.pdf || form.csv;
	const ready = form.models.length > 0 && hasFormat && !view.tooMany;
	return (
		<Group justify="space-between" wrap="nowrap">
			<Text c="stone.7" size="xs" truncate>
				{account && t('desktop.oneOff.account', { name: account.displayName })}
			</Text>
			<Group gap="sm" wrap="nowrap">
				<Button loading={actions.cancel.isPending} onClick={() => actions.cancel.mutate()} variant="default">
					{t('desktop.oneOff.cancel')}
				</Button>
				<Button disabled={!ready} loading={actions.analyze.isPending} onClick={() => actions.analyze.mutate()}>
					{t('desktop.oneOff.analyze', { count: view.count })}
				</Button>
			</Group>
		</Group>
	);
}

function Body({ view, catalogue }: { view: OneOffView; catalogue: CatalogueView | undefined }) {
	const { t } = useTranslation();
	const holds = useQuery(holdsQuery).data?.holds ?? [];
	const [form, setForm] = useState<Form>(() => ({
		label: view.label,
		models: initialModels(view, catalogue),
		pdf: true,
		csv: false,
		language: view.language,
		reports: null,
		open: false,
	}));
	const update = (changes: Partial<Form>) => setForm((current) => ({ ...current, ...changes }));
	const actions = useActions(form);
	return (
		<Stack gap="md">
			{view.tooMany && <Notice tone="error">{t('desktop.oneOff.tooMany')}</Notice>}
			{holds.length > 0 && <Notice tone="warn">{t('desktop.oneOff.holdNote')}</Notice>}
			<Group align="flex-start" gap="lg" grow wrap="nowrap">
				<Stack gap="sm">
					<ImageList view={view} />
					<TextInput
						data-autofocus
						label={t('desktop.oneOff.name')}
						maxLength={200}
						onChange={(event) => update({ label: event.currentTarget.value })}
						value={form.label}
					/>
					<CostLine count={view.count} />
				</Stack>
				<Stack gap="sm">
					<Models form={form} update={update} view={view} />
					<ReportChoice form={form} update={update} view={view} />
				</Stack>
			</Group>
			<Footer actions={actions} form={form} view={view} />
		</Stack>
	);
}

function Nothing({ view }: { view: OneOffView }) {
	const { t } = useTranslation();
	const close = useMutation({ mutationFn: () => commands.discardOneOff(null) });
	return (
		<Stack gap="md">
			<Text c="stone.8" size="sm">
				{t('desktop.oneOff.noneText')}
			</Text>
			<NotIncludedList view={view} />
			<Group justify="flex-end">
				<Button data-autofocus loading={close.isPending} onClick={() => close.mutate()}>
					{t('desktop.oneOff.close')}
				</Button>
			</Group>
		</Stack>
	);
}

export function OneOffDialog({ view }: { view: OneOffView }) {
	const { t } = useTranslation();
	const catalogue = useQuery(catalogueQuery);
	const cancel = useCancel();
	const empty = view.count === 0;
	const title = (
		<Text c="jade.8" component="span" fw={700} size="lg">
			{empty ? t('desktop.oneOff.noneTitle') : t('desktop.oneOff.title', { count: view.count })}
		</Text>
	);
	return (
		<Modal
			centered
			closeButtonProps={{ 'aria-label': t('desktop.oneOff.cancel') }}
			onClose={() => cancel.mutate()}
			opened={empty || catalogue.isFetched}
			size="xl"
			title={title}
			yOffset="2dvh"
		>
			{empty && <Nothing view={view} />}
			{!empty && catalogue.isFetched && <Body catalogue={catalogue.data} view={view} />}
		</Modal>
	);
}
