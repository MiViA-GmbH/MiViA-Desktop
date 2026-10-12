import { Anchor, Divider, List, Stack, Text } from '@mantine/core';
import { useQuery } from '@tanstack/react-query';
import type { TFunction } from 'i18next';
import { Fragment, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { localeQuery } from '../../entities/account/queries';
import { analysisName } from '../../entities/series/status';
import type { CustomAnalysisView, CustomReportDraft } from '../../shared/ipc/bindings';

const REASONS = {
	FAILED: 'desktop.custom.reasons.FAILED',
	REJECTED: 'desktop.custom.reasons.REJECTED',
	UNAVAILABLE: 'desktop.custom.reasons.UNAVAILABLE',
	LOST: 'desktop.custom.reasons.LOST',
	CANCELLED: 'desktop.custom.reasons.CANCELLED',
} as const;

function reasonText(reason: string, t: TFunction): string {
	return reason in REASONS ? t(REASONS[reason as keyof typeof REASONS]) : reason;
}

function NotIncluded({ analysis }: { analysis: CustomAnalysisView }) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	if (analysis.omitted.length === 0) {
		return null;
	}
	return (
		<>
			<Anchor
				c="stone.8"
				component="button"
				onClick={() => setOpen(!open)}
				size="xs"
				ta="left"
				type="button"
				underline="always"
				w="fit-content"
			>
				{t('desktop.custom.notIncluded', { count: analysis.omitted.length })}
			</Anchor>
			{open && (
				<List size="xs" spacing={2}>
					{analysis.omitted.map((entry) => (
						<List.Item key={`${entry.label}-${entry.reason}`}>
							{entry.label}: {reasonText(entry.reason, t)}
						</List.Item>
					))}
				</List>
			)}
		</>
	);
}

export function DraftSummary({ draft }: { draft: CustomReportDraft }) {
	const { t } = useTranslation();
	const language = useQuery(localeQuery).data?.language ?? 'en';
	const reports = draft.analyses.filter((analysis) => analysis.included > 0);
	return (
		<Stack gap="xs">
			<Text c="stone.8" size="sm">
				{t('desktop.custom.creates', { count: reports.length })}
			</Text>
			{draft.analyses.map((analysis, index) => (
				<Fragment key={analysis.analysis.key}>
					{index > 0 && <Divider />}
					<Stack gap={2}>
						<Text fw={500} size="sm">
							{analysisName(analysis.analysis, language)}
						</Text>
						<Text c="stone.7" size="xs">
							{t('desktop.custom.included', { count: analysis.included })}
							{analysis.parts > 1 && ` · ${t('desktop.custom.parts', { count: analysis.parts })}`}
						</Text>
						<NotIncluded analysis={analysis} />
					</Stack>
				</Fragment>
			))}
		</Stack>
	);
}
