import { createRootRoute, createRoute, createRouter } from '@tanstack/react-router';

import { NotFound, RouteError } from '../routes/errors';
import { Shell } from './shell/app-shell';

const root = createRootRoute({ component: Shell, notFoundComponent: NotFound, errorComponent: RouteError });

const overview = createRoute({ getParentRoute: () => root, path: '/' }).lazy(() =>
	import('../routes/overview.lazy').then((module) => module.OverviewRoute),
);

const history = createRoute({ getParentRoute: () => root, path: '/history' }).lazy(() =>
	import('../routes/history.lazy').then((module) => module.HistoryRoute),
);

const singleImages = createRoute({ getParentRoute: () => root, path: '/single-images' }).lazy(() =>
	import('../routes/history.lazy').then((module) => module.SingleImagesRoute),
);

const folder = createRoute({ getParentRoute: () => root, path: '/folders/$folderId' }).lazy(() =>
	import('../routes/folder.lazy').then((module) => module.FolderSeriesRoute),
);

const folderSettings = createRoute({ getParentRoute: () => root, path: '/folders/$folderId/settings' }).lazy(() =>
	import('../routes/folder.lazy').then((module) => module.FolderSettingsRoute),
);

const series = createRoute({ getParentRoute: () => root, path: '/series/$seriesId' }).lazy(() =>
	import('../routes/series.lazy').then((module) => module.SeriesRoute),
);

const settings = createRoute({ getParentRoute: () => root, path: '/settings/$section' }).lazy(() =>
	import('../routes/settings.lazy').then((module) => module.SettingsRoute),
);

const tree = root.addChildren([overview, history, singleImages, folder, folderSettings, series, settings]);

export const router = createRouter({ routeTree: tree, defaultPreload: false });

declare module '@tanstack/react-router' {
	interface Register {
		router: typeof router;
	}
}
