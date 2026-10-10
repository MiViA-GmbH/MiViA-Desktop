import { Button, Loader, Stack, Text } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import { useEffect, useEffectEvent, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { catalogueQuery } from '../../entities/setup/queries';
import type { CatalogueView, SetupView } from '../../shared/ipc/bindings';
import { Notice } from '../../shared/ui/notice';
import { ModelChoices } from './model-choices';
import { previousStep } from './steps';
import { useAdvance } from './use-advance';
import { useGo, useSave } from './use-setup';
import { WizardFrame } from './wizard-frame';

type CatalogueProps = { view: SetupView; catalogue: CatalogueView; error: string | null; onRetry: () => void };

function Catalogue({ view, catalogue, error, onRetry }: CatalogueProps) {
	const { t } = useTranslation();
	const { change } = useSave(view);
	if (!catalogue.loaded) {
		return (
			<Notice tone="warn">
				<Stack align="flex-start" gap="xs">
					<Text inherit>{t('desktop.wizard.models.loadFailed')}</Text>
					<Button onClick={onRetry} size="xs" variant="default">
						{t('desktop.wizard.common.tryAgain')}
					</Button>
				</Stack>
			</Notice>
		);
	}
	if (catalogue.models.length === 0) {
		return <Notice tone="warn">{t('desktop.wizard.models.none', { contact: view.supportContact })}</Notice>;
	}
	return (
		<ModelChoices
			catalogue={catalogue}
			chosen={view.models}
			contact={view.supportContact}
			error={error}
			host={view.host}
			onChange={(models) => change({ models })}
		/>
	);
}

export function ModelsStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { label, loading, advance } = useAdvance(view, 'models');
	const { change } = useSave(view);
	const catalogue = useQuery(catalogueQuery);
	const [missing, setMissing] = useState(false);
	const preselect = useEffectEvent((loaded: CatalogueView) => {
		if (view.models.length === 0 && loaded.preselected.length > 0) {
			change({ models: loaded.preselected });
		}
	});
	useEffect(() => {
		if (catalogue.data) {
			preselect(catalogue.data);
		}
	}, [catalogue.data]);
	const needed = (catalogue.data?.models.length ?? 0) > 0;
	const next = () => {
		setMissing(true);
		if (catalogue.data && (!needed || view.models.length > 0)) {
			advance();
		}
	};
	return (
		<WizardFrame
			onBack={() => go(previousStep(view, 'models'))}
			primary={{ label, onClick: next, loading }}
			step="models"
			title={t('desktop.wizard.models.title')}
			view={view}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.models.lead')}
			</Text>
			{catalogue.data ? (
				<Catalogue
					catalogue={catalogue.data}
					onRetry={() => catalogue.refetch()}
					error={missing && view.models.length === 0 ? t('desktop.wizard.models.required') : null}
					view={view}
				/>
			) : (
				<Loader aria-label={t('desktop.wizard.models.loading')} size="sm" />
			)}
		</WizardFrame>
	);
}
