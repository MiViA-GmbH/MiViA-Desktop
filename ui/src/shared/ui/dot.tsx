import { Box, type BoxProps } from '@mantine/core';

export function Dot({ label, ...props }: BoxProps & { label?: string }) {
	return (
		<Box
			aria-label={label}
			bg="currentColor"
			h={6}
			role={label ? 'img' : undefined}
			style={{ borderRadius: '50%' }}
			w={6}
			{...props}
		/>
	);
}
