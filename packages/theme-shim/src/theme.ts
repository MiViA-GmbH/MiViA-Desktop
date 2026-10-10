import type {
	DefaultMantineColor,
	MantineColorsTuple,
	MantineThemeOverride,
} from '@mantine/core';

type ExtendedCustomColors = 'jade' | 'forest' | 'stone' | DefaultMantineColor;

declare module '@mantine/core' {
	export interface MantineThemeColorsOverride {
		colors: Record<ExtendedCustomColors, MantineColorsTuple>;
	}
}

export const theme: MantineThemeOverride = {
	colors: {
		jade: [
			'#deffff',
			'#b3faff',
			'#86f6fe',
			'#5cf2fd',
			'#40effd',
			'#35d5e4',
			'#24a6b1',
			'#15767f',
			'#00393d', // wg specyfikacji
			'#00191c',
		],
		forest: [
			'#e6fae6',
			'#c7eac5',
			'#a5daa2',
			'#82cc7f',
			'#60bd5b',
			'#49a944', // wg specyfikacji
			'#367f32',
			'#255b23',
			'#143813',
			'#001400',
		],
		stone: [
			'#ffffff',
			'#f0f0f0', // wg specyfikacji
			'#ddd7d9',
			'#c1bfbf',
			'#b2b2b2', // wg specyfikacji
			'#8c8c8c',
			'#737373',
			'#595959',
			'#413f40',
			'#292526',
		],
	},
	components: {
		Loader: {
			defaultProps: { color: '#00393d' },
		},
		ActionIcon: {
			defaultProps: { color: 'jade' },
		},
	},
	primaryColor: 'jade',
	defaultRadius: 'sm',
	fontFamily: 'Space Grotesk, sans-serif',
	fontFamilyMonospace: 'Space Grotesk, Courier, monospace',
	headings: { fontFamily: 'Barlow, sans-serif' },
};

export const ICON_STYLE = { width: '70%', height: '70%' };
export const ICON_STROKE = 1.5;
