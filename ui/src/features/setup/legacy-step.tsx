import { Anchor, Button, Checkbox, Code, Group, List, Paper, Text, ThemeIcon } from '@mantine/core';
import { ICON_STROKE } from '@mivia-ai/theme';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Trans, useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { LegacyTake, LegacyView, SetupView } from '../../shared/ipc/bindings';
import { CheckIcon } from '../../shared/ui/icons';
import { Notice } from '../../shared/ui/notice';
import { has, nextStep, previousStep } from './steps';
import { IMPORT_KEY } from './use-advance';
import { useGo, useSave, useSetupView, withOption } from './use-setup';
import { WizardFrame } from './wizard-frame';

function Found({ legacy }: { legacy: LegacyView }) {
	const { t } = useTranslation();
	const check = (
		<ThemeIcon color="forest" radius="xl" size={20} variant="light">
			<CheckIcon size={12} stroke={ICON_STROKE} />
		</ThemeIcon>
	);
	return (
		<Paper bg="stone.1" p="sm" withBorder>
			<List center icon={check} size="sm" spacing={6}>
				{legacy.flags.includes('hasKey') && <List.Item>{t('desktop.wizard.legacy.hasKey')}</List.Item>}
				{legacy.inputFolder && (
					<List.Item>
						<Trans
							components={{ code: <Code /> }}
							i18nKey="desktop.wizard.legacy.hasFolder"
							values={{ path: legacy.inputFolder }}
						/>
					</List.Item>
				)}
				{legacy.modelCount > 0 && (
					<List.Item>{t('desktop.wizard.legacy.hasModels', { count: legacy.modelCount })}</List.Item>
				)}
			</List>
		</Paper>
	);
}

function Running({ legacy }: { legacy: LegacyView }) {
	const { t } = useTranslation();
	const store = useSetupView();
	const close = useMutation({ mutationFn: () => commands.closeLegacyApp(), onSuccess: store });
	if (!legacy.flags.includes('running')) {
		return null;
	}
	return (
		<Notice tone="warn">
			<Group justify="space-between">
				{t('desktop.wizard.legacy.running')}
				<Button loading={close.isPending} onClick={() => close.mutate()} size="xs" variant="default">
					{t('desktop.wizard.legacy.closeIt')}
				</Button>
			</Group>
		</Notice>
	);
}

function useTake(view: SetupView) {
	const client = useQueryClient();
	const { change } = useSave(view);
	const go = useGo(view);
	return useMutation({
		mutationFn: async (take: LegacyTake) => {
			await change({ legacyTake: take });
			const imported = take === 'nothing' ? null : await commands.importLegacyKey();
			client.setQueryData(IMPORT_KEY, imported);
			await go(nextStep(view, 'legacy'));
		},
	});
}

export function LegacyStep({ view, legacy }: { view: SetupView; legacy: LegacyView }) {
	const { t } = useTranslation();
	const { change } = useSave(view);
	const store = useSetupView();
	const go = useGo(view);
	const take = useTake(view);
	const pick = useMutation({ mutationFn: () => commands.pickSetupFolder('legacy'), onSuccess: store });
	const network = legacy.flags.includes('network');
	const other = legacy.flags.includes('hasKey') ? 'keyOnly' : 'nothing';
	return (
		<WizardFrame
			onBack={() => go(previousStep(view, 'legacy'))}
			primary={{
				label: t('desktop.wizard.legacy.takeAll'),
				onClick: () => take.mutate('all'),
				loading: take.isPending,
			}}
			step="legacy"
			title={t('desktop.wizard.legacy.title')}
			view={view}
		>
			<Text c="stone.8" size="sm">
				<Trans components={{ code: <Code /> }} i18nKey="desktop.wizard.legacy.found" values={{ path: legacy.folder }} />
			</Text>
			{legacy.flags.includes('unreadable') ? (
				<Notice tone="warn">{t('desktop.wizard.legacy.unreadable')}</Notice>
			) : (
				<Found legacy={legacy} />
			)}
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.legacy.safe')}
			</Text>
			{network ? (
				<Notice tone="info">{t('desktop.wizard.legacy.stopNetwork')}</Notice>
			) : (
				<Checkbox
					checked={has(view, 'legacyStop')}
					label={t('desktop.wizard.legacy.stop')}
					onChange={(event) => change({ options: withOption(view.options, 'legacyStop', event.currentTarget.checked) })}
				/>
			)}
			<Running legacy={legacy} />
			<Button onClick={() => take.mutate(other)} size="xs" variant="default" w="fit-content">
				{other === 'keyOnly' ? t('desktop.wizard.legacy.keyOnly') : t('desktop.wizard.legacy.none')}
			</Button>
			<Anchor component="button" onClick={() => pick.mutate()} size="sm" ta="left" type="button" w="fit-content">
				{t('desktop.wizard.legacy.other')}
			</Anchor>
		</WizardFrame>
	);
}
