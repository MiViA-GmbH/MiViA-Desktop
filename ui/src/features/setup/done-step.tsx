import { List, Stack, Text, ThemeIcon, Title } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { Trans, useTranslation } from 'react-i18next';

import { catalogueQuery } from '../../entities/setup/queries';
import type { DoneView, SetupView, WaitingReason } from '../../shared/ipc/bindings';
import { CheckIcon } from '../../shared/ui/icons';
import { Notice } from '../../shared/ui/notice';
import { WizardFrame } from './wizard-frame';

const REASONS = {
	noModels: 'desktop.wizard.done.reasonNoModels',
	license: 'desktop.wizard.done.reasonLicense',
	offline: 'desktop.wizard.done.reasonOffline',
} as const satisfies Record<WaitingReason, string>;

function Summary({ done }: { done: DoneView }) {
	const { t } = useTranslation();
	const names = useQuery({ ...catalogueQuery, enabled: false }).data?.models ?? [];
	const models = done.models.map((id) => names.find((model) => model.modelId === id)?.name ?? id).join(', ');
	const check = (
		<ThemeIcon color="forest" radius="xl" size={20} variant="light">
			<CheckIcon size={12} stroke={ICON_STROKE} />
		</ThemeIcon>
	);
	return (
		<List icon={check} size="sm" spacing={6}>
			<List.Item>{t('desktop.wizard.key.connected', { name: done.name, email: done.email })}</List.Item>
			{done.folderName && (
				<List.Item>
					<Trans
						components={{ b: <b /> }}
						i18nKey="desktop.wizard.done.watching"
						values={{ folder: done.folderName }}
					/>
				</List.Item>
			)}
			{models && <List.Item>{t('desktop.wizard.done.models', { models })}</List.Item>}
			{done.reportFolder && <List.Item>{t('desktop.wizard.done.reports', { folder: done.reportFolder })}</List.Item>}
			{done.legacy === 'stopped' && <List.Item>{t('desktop.wizard.done.legacyStopped')}</List.Item>}
		</List>
	);
}

function Next({ done }: { done: DoneView }) {
	const { t } = useTranslation();
	return (
		<Stack gap={6}>
			<Title order={4}>{t('desktop.wizard.done.nextTitle')}</Title>
			{done.folderName && (
				<Text size="sm">
					<Trans components={{ b: <b /> }} i18nKey="desktop.wizard.done.how" values={{ folder: done.folderName }} />
				</Text>
			)}
			{done.existing > 0 && <Text size="sm">{t('desktop.wizard.done.existing', { count: done.existing })}</Text>}
			{done.explorer && <Text size="sm">{t('desktop.wizard.done.single')}</Text>}
			<Text size="sm">{t('desktop.wizard.done.tray')}</Text>
			<Text c="stone.7" size="sm">
				{t('desktop.wizard.done.perUser')}
			</Text>
		</Stack>
	);
}

function Problems({ done }: { done: DoneView }) {
	const { t } = useTranslation();
	return (
		<>
			{done.problems.includes('autostart') && <Notice tone="warn">{t('desktop.wizard.done.autostartFailed')}</Notice>}
			{done.problems.includes('explorer') && <Notice tone="warn">{t('desktop.wizard.done.explorerFailed')}</Notice>}
			{done.legacy === 'close' && <Notice tone="warn">{t('desktop.wizard.done.legacyClose')}</Notice>}
			{done.legacy === 'failed' && <Notice tone="warn">{t('desktop.wizard.done.legacyFailed')}</Notice>}
			{done.legacyStartup.map((path) => (
				<Notice key={path} tone="info">
					{t('desktop.wizard.done.legacyStartup', { path })}
				</Notice>
			))}
		</>
	);
}

export function DoneStep({ view, done }: { view: SetupView; done: DoneView }) {
	const { t } = useTranslation();
	const navigate = useNavigate();
	const waiting = done.waiting.length > 0;
	return (
		<WizardFrame
			primary={{ label: t('desktop.wizard.done.open'), onClick: () => navigate({ to: '/' }) }}
			step="done"
			title={waiting ? t('desktop.wizard.done.titleWaiting') : t('desktop.wizard.done.title')}
			view={view}
		>
			{waiting && (
				<Notice tone="warn">
					{done.waiting.map((reason) => t(REASONS[reason])).join(' ')} {t('desktop.wizard.done.waitingSafe')}
				</Notice>
			)}
			<Summary done={done} />
			<Next done={done} />
			<Problems done={done} />
		</WizardFrame>
	);
}
