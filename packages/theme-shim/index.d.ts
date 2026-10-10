import type {
	CSSVariablesResolver,
	DefaultMantineColor,
	MantineColorsTuple,
	MantineThemeOverride,
} from '@mantine/core';
import type { ComponentPropsWithoutRef, ReactElement } from 'react';

type ExtendedCustomColors = 'jade' | 'forest' | 'stone' | DefaultMantineColor;

declare module '@mantine/core' {
	export interface MantineThemeColorsOverride {
		colors: Record<ExtendedCustomColors, MantineColorsTuple>;
	}
}

interface MiviaLogoProps extends ComponentPropsWithoutRef<'svg'> {
	variant?: 'white' | 'default';
	w?: number;
}

export declare const theme: MantineThemeOverride;
export declare const ICON_STROKE: number;
export declare const v8CssVariablesResolver: CSSVariablesResolver;
export declare function MiviaLogo(props: MiviaLogoProps): ReactElement;
export declare function MiviaLogoClaimBelow(props: MiviaLogoProps): ReactElement;
