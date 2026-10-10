import {
	Alert,
	Anchor,
	Badge,
	createTheme,
	Divider,
	mergeThemeOverrides,
	Notification,
	Table,
	Title,
} from '@mantine/core';
import { theme } from '@mivia-ai/theme';

export const desktopTheme = mergeThemeOverrides(
	theme,
	createTheme({
		components: {
			Anchor: Anchor.extend({ defaultProps: { c: 'forest.7' } }),
			Alert: Alert.extend({ defaultProps: { variant: 'light' } }),
			Badge: Badge.extend({ defaultProps: { variant: 'light', tt: 'none', size: 'md' } }),
			Divider: Divider.extend({ defaultProps: { color: 'stone.2' } }),
			Notification: Notification.extend({ defaultProps: { withBorder: true, color: 'jade' } }),
			Table: Table.extend({ defaultProps: { verticalSpacing: 8 } }),
			Title: Title.extend({ defaultProps: { c: 'jade.8' } }),
		},
	}),
);
