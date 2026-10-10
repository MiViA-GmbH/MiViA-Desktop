import { Text, type TextProps } from '@mantine/core';

import classes from './path.module.css';

export function PathText({ children, ...props }: TextProps & { children: string }) {
	return (
		<Text classNames={classes} {...props}>
			{children}
		</Text>
	);
}
