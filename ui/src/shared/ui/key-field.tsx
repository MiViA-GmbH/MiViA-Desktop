import { PasswordInput } from '@mantine/core';
import type { Ref } from 'react';
import { useTranslation } from 'react-i18next';

type KeyFieldProps = {
	value: string;
	error: string | null;
	inputRef: Ref<HTMLInputElement>;
	onChange: (value: string) => void;
	onPaste?: ((text: string) => void) | undefined;
};

export function KeyField({ value, error, inputRef, onChange, onPaste }: KeyFieldProps) {
	const { t } = useTranslation();
	return (
		<PasswordInput
			autoComplete="off"
			error={error}
			label={t('desktop.wizard.key.label')}
			onChange={(event) => onChange(event.currentTarget.value)}
			onPaste={(event) => onPaste?.(event.clipboardData.getData('text'))}
			placeholder="mv-…"
			ref={inputRef}
			size="md"
			spellCheck={false}
			value={value}
		/>
	);
}
