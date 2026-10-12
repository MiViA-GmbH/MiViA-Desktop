import { Checkbox, Group, Select, Stack, Text, TextInput } from '@mantine/core';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { CatalogueModelView, CatalogueView, ModelChoiceView } from '../../shared/ipc/bindings';
import { Notice } from '../../shared/ui/notice';

const SEARCH_ABOVE = 8;
const STANDARD = '';

type ModelChoicesProps = {
	catalogue: CatalogueView;
	chosen: ModelChoiceView[];
	host: string;
	contact: string;
	error: string | null;
	onChange: (chosen: ModelChoiceView[]) => void;
	compact?: boolean;
};

type RowProps = {
	model: CatalogueModelView;
	choice: ModelChoiceView | undefined;
	host: string;
	customError: string | null;
	compact: boolean;
	onToggle: (on: boolean) => void;
	onCustomization: (id: string | null) => void;
};

function ModelRow({ model, choice, host, customError, compact, onToggle, onCustomization }: RowProps) {
	const { t } = useTranslation();
	const options = [
		{ value: STANDARD, label: t('desktop.wizard.models.standard') },
		...model.customizations.map((custom) => ({ value: custom.id, label: custom.name })),
	];
	return (
		<Stack gap={0}>
			{compact ? (
				<Checkbox
					checked={choice !== undefined}
					label={model.name}
					onChange={(event) => onToggle(event.currentTarget.checked)}
				/>
			) : (
				<Checkbox.Card checked={choice !== undefined} onClick={() => onToggle(choice === undefined)} p="sm">
					<Group align="flex-start" gap="sm" wrap="nowrap">
						<Checkbox.Indicator mt={2} />
						<Stack gap={2}>
							<Text fw={600} size="sm">
								{model.name}
							</Text>
							{model.description && (
								<Text c="stone.7" size="xs">
									{model.description}
								</Text>
							)}
						</Stack>
					</Group>
				</Checkbox.Card>
			)}
			{choice && model.customizations.length > 0 && (
				<Select
					allowDeselect={false}
					data={options}
					description={compact ? undefined : t('desktop.wizard.models.customizationHint', { host })}
					error={customError}
					label={t('desktop.wizard.models.customization')}
					maw={360}
					ml={compact ? 32 : 44}
					mt="xs"
					onChange={(value) => onCustomization(value === STANDARD ? null : value)}
					placeholder={t('desktop.wizard.models.choosePlaceholder')}
					size="xs"
					value={customError ? null : (choice.customizationId ?? STANDARD)}
				/>
			)}
		</Stack>
	);
}

export function ModelChoices({
	catalogue,
	chosen,
	host,
	contact,
	error,
	onChange,
	compact = false,
}: ModelChoicesProps) {
	const { t } = useTranslation();
	const [search, setSearch] = useState('');
	const [resolved, setResolved] = useState<string[]>([]);
	const shown = catalogue.models.filter((model) => model.name.toLowerCase().includes(search.trim().toLowerCase()));
	const toggle = (model: CatalogueModelView, on: boolean) => {
		const rest = chosen.filter((choice) => choice.modelId !== model.modelId);
		onChange(on ? [...rest, { modelId: model.modelId, customizationId: null }] : rest);
	};
	const customize = (model: CatalogueModelView, id: string | null) => {
		setResolved([...resolved, model.modelId]);
		onChange(chosen.map((choice) => (choice.modelId === model.modelId ? { ...choice, customizationId: id } : choice)));
	};
	const gone = (model: CatalogueModelView) =>
		catalogue.legacyCustomGone.includes(model.modelId) && !resolved.includes(model.modelId);
	return (
		<Stack gap="xs">
			{catalogue.legacyGone.map((name) => (
				<Notice key={name} tone="warn">
					{t('desktop.wizard.models.legacyGone', { name, contact })}
				</Notice>
			))}
			{catalogue.models.length > SEARCH_ABOVE && (
				<TextInput
					label={t('desktop.wizard.models.search')}
					onChange={(event) => setSearch(event.currentTarget.value)}
					value={search}
				/>
			)}
			{shown.map((model) => (
				<ModelRow
					choice={chosen.find((choice) => choice.modelId === model.modelId)}
					compact={compact}
					customError={gone(model) ? t('desktop.wizard.models.legacyCustomGone', { name: model.name }) : null}
					host={host}
					key={model.modelId}
					model={model}
					onCustomization={(id) => customize(model, id)}
					onToggle={(on) => toggle(model, on)}
				/>
			))}
			{error && (
				<Text c="red.7" role="alert" size="sm">
					{error}
				</Text>
			)}
		</Stack>
	);
}
