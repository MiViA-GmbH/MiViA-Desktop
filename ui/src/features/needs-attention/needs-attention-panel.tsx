import { Anchor, Badge } from '@mantine/core';
import { Link } from '@tanstack/react-router';
import { useTranslation } from 'react-i18next';

import type { NeedsAttentionView } from '../../shared/ipc/bindings';
import { Panel } from '../../shared/ui/panel';
import { AttentionRows } from './attention-rows';

const SHOWN = 5;

export function NeedsAttentionPanel({ view }: { view: NeedsAttentionView }) {
	const { t } = useTranslation();
	if (view.items.length === 0) {
		return null;
	}
	return (
		<Panel
			extra={
				<Badge circle color="red">
					{view.total}
				</Badge>
			}
			title={t('desktop.attention.title')}
		>
			<AttentionRows items={view.items.slice(0, SHOWN)} />
			{view.total > SHOWN && (
				<Anchor component={Link} size="sm" to="/history">
					{t('desktop.attention.showAll', { count: view.total })}
				</Anchor>
			)}
		</Panel>
	);
}
