import { Checkbox, Group } from '@mantine/core';
import { useTranslation } from 'react-i18next';

export type Formats = { pdf: boolean; csv: boolean };

export function FormatChoice({ form, update }: { form: Formats; update: (changes: Partial<Formats>) => void }) {
	const { t } = useTranslation();
	return (
		<Group gap="lg">
			<Checkbox checked={form.pdf} label="PDF" onChange={(event) => update({ pdf: event.currentTarget.checked })} />
			<Checkbox
				checked={form.csv}
				label={t('desktop.oneOff.formatCsv')}
				onChange={(event) => update({ csv: event.currentTarget.checked })}
			/>
		</Group>
	);
}
