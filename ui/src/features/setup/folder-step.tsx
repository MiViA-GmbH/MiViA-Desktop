import { Anchor, Text } from '@mantine/core';
import { useMutation } from '@tanstack/react-query';
import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { commands } from '../../shared/ipc';
import type { SetupView } from '../../shared/ipc/bindings';
import { Notice } from '../../shared/ui/notice';
import { FolderChoice, FolderDetails } from './folder-choice';
import { blockText } from './folder-text';
import { has, nextStep, previousStep } from './steps';
import { useAdvance } from './use-advance';
import { useGo, useSave, useSetupView, withOption } from './use-setup';
import { WizardFrame } from './wizard-frame';

function NoFolder({ onSkip }: { onSkip: () => void }) {
	const { t } = useTranslation();
	return (
		<>
			<Text c="stone.7" size="sm">
				{t('desktop.wizard.folder.notSure')}
			</Text>
			<Anchor component="button" onClick={onSkip} size="sm" ta="left" type="button" w="fit-content">
				{t('desktop.wizard.folder.skip')}
			</Anchor>
		</>
	);
}

function useFolderStep(view: SetupView) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { label, loading, advance } = useAdvance(view, 'folder');
	const { change } = useSave(view);
	const store = useSetupView();
	const [missing, setMissing] = useState(false);
	const choose = useRef<HTMLButtonElement>(null);
	const pick = useMutation({ mutationFn: () => commands.pickSetupFolder('input'), onSuccess: store });
	const folder = view.folder;
	const blocked = folder?.block ? blockText(folder.block, t) : null;
	const error = blocked ?? (missing && !folder ? t('desktop.wizard.folder.required') : null);
	const next = () => {
		setMissing(true);
		if (!folder || folder.block) {
			choose.current?.focus();
			return;
		}
		advance();
	};
	const skip = async () => {
		const options = withOption(view.options, 'skipFolder', true);
		await change({ options });
		await go(nextStep({ ...view, options }, 'folder'));
	};
	return { pick, choose, error, next, skip, change, label, loading };
}

export function FolderStep({ view }: { view: SetupView }) {
	const { t } = useTranslation();
	const go = useGo(view);
	const { pick, choose, error, next, skip, change, label, loading } = useFolderStep(view);
	const folder = view.folder;
	return (
		<WizardFrame
			onBack={() => go(previousStep(view, 'folder'))}
			primary={{ label, onClick: next, loading }}
			step="folder"
			title={t('desktop.wizard.folder.title')}
			view={view}
		>
			<Text c="stone.8" size="sm">
				{t('desktop.wizard.folder.lead')}
			</Text>
			{view.legacyFolderMissing && (
				<Notice tone="warn">{t('desktop.wizard.folder.legacyMissing', { path: view.legacyFolderMissing })}</Notice>
			)}
			<FolderChoice
				busy={pick.isPending}
				chooseRef={choose}
				error={error}
				folder={folder}
				onChoose={() => pick.mutate()}
			/>
			<FolderDetails
				folder={folder}
				include={has(view, 'includeExisting')}
				onInclude={(include) => change({ options: withOption(view.options, 'includeExisting', include) })}
				remaining={view.connection?.remainingUploads ?? null}
			/>
			{!folder && <NoFolder onSkip={skip} />}
		</WizardFrame>
	);
}
