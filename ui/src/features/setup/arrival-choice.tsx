import { Button, Collapse, Divider, Group, Radio, Stack, Text } from '@mantine/core';
import { useDisclosure } from '@mantine/hooks';
import { useTranslation } from 'react-i18next';

import type { ArrivalPreset, ReportFolderView, ReportMode } from '../../shared/ipc/bindings';
import { Notice } from '../../shared/ui/notice';
import { PathText } from '../../shared/ui/path-text';

const PRESETS = {
	microscope: { label: 'desktop.wizard.series.microscope', hint: 'desktop.wizard.series.microscopeHint' },
	wholeFolder: { label: 'desktop.wizard.series.wholeFolder', hint: 'desktop.wizard.series.wholeFolderHint' },
	byHand: { label: 'desktop.wizard.series.byHand', hint: 'desktop.wizard.series.byHandHint' },
} as const;

const PRESET_ORDER: ArrivalPreset[] = ['microscope', 'wholeFolder', 'byHand'];

function isPreset(value: string): value is ArrivalPreset {
	return PRESET_ORDER.some((preset) => preset === value);
}

type PresetChoiceProps = { preset: ArrivalPreset; onChange: (preset: ArrivalPreset) => void };

export function PresetChoice({ preset, onChange }: PresetChoiceProps) {
	const { t } = useTranslation();
	return (
		<Radio.Group
			aria-label={t('desktop.wizard.series.title')}
			onChange={(value) => isPreset(value) && onChange(value)}
			value={preset}
		>
			<Stack gap="xs">
				{PRESET_ORDER.map((key) => (
					<Radio.Card key={key} px="sm" py="xs" value={key}>
						<Group align="flex-start" gap="sm" wrap="nowrap">
							<Radio.Indicator mt={2} />
							<Stack gap={2}>
								<Text fw={600} size="sm">
									{t(PRESETS[key].label)}
								</Text>
								<Text c="stone.7" size="xs">
									{t(PRESETS[key].hint)}
								</Text>
							</Stack>
						</Group>
					</Radio.Card>
				))}
			</Stack>
		</Radio.Group>
	);
}

const MODES: ReportMode[] = ['subfolder', 'beside', 'custom'];

function isMode(value: string): value is ReportMode {
	return MODES.some((mode) => mode === value);
}

type ReportChoiceProps = {
	report: ReportFolderView;
	legacy: boolean;
	onMode: (mode: ReportMode) => void;
	onPickCustom: () => void;
};

export function ReportChoice({ report, legacy, onMode, onPickCustom }: ReportChoiceProps) {
	const { t } = useTranslation();
	const [open, { toggle }] = useDisclosure(false);
	const choose = (value: string) => isMode(value) && value !== 'custom' && onMode(value);
	return (
		<Stack gap={6}>
			<Divider />
			<Text fw={600} size="sm">
				{t('desktop.wizard.series.reportsIn')}
			</Text>
			<Group gap="sm" wrap="nowrap">
				<PathText flex={1} size="sm">
					{report.display}
				</PathText>
				<Button aria-expanded={open} onClick={toggle} size="xs" variant="subtle">
					{t('desktop.wizard.series.reportsChange')}
				</Button>
			</Group>
			<Collapse expanded={open}>
				<Radio.Group aria-label={t('desktop.wizard.series.reportsIn')} onChange={choose} value={report.mode}>
					<Stack gap="xs" mt="xs">
						<Radio label={t('desktop.wizard.series.modeSubfolder', { name: report.subfolderName })} value="subfolder" />
						<Radio
							label={
								legacy
									? `${t('desktop.wizard.series.modeBeside')} (${t('desktop.wizard.series.asBefore')})`
									: t('desktop.wizard.series.modeBeside')
							}
							value="beside"
						/>
						<Radio label={t('desktop.wizard.series.modeCustom')} onClick={onPickCustom} value="custom" />
					</Stack>
				</Radio.Group>
			</Collapse>
			{report.denied && (
				<Notice tone="warn">
					{t('desktop.wizard.series.probeDenied', { folder: report.denied, fallback: report.display })}
				</Notice>
			)}
			{report.longPath && <Notice tone="warn">{t('desktop.wizard.series.longPath')}</Notice>}
		</Stack>
	);
}
