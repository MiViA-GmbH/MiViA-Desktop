import {
	createRootRoute,
	createRoute,
	createRouter,
	lazyRouteComponent,
	Outlet,
	redirect,
} from '@tanstack/react-router';

import { accountQuery } from '../entities/account/queries';
import { parseHistorySearch } from '../entities/series/search';
import { setupQuery } from '../entities/setup/queries';
import { NotFound, RouteError } from '../routes/errors';
import { loadSetupTexts } from '../shared/i18n/i18n';
import { queryClient } from './query-client';

const root = createRootRoute({ component: Outlet, notFoundComponent: NotFound, errorComponent: RouteError });

const app = createRoute({
	getParentRoute: () => root,
	id: 'app',
	component: lazyRouteComponent(() => import('./shell/app-shell').then((module) => ({ default: module.Shell }))),
	beforeLoad: async ({ location }) => {
		const setup = await queryClient.ensureQueryData(setupQuery);
		if (!setup.completed) {
			throw redirect({ to: '/welcome/$step', params: { step: setup.step } });
		}
		const account = await queryClient.ensureQueryData(accountQuery);
		if (!account && location.pathname !== '/settings/account') {
			throw redirect({ to: '/settings/$section', params: { section: 'account' } });
		}
	},
});

const welcome = createRoute({ getParentRoute: () => root, path: '/welcome/$step', loader: loadSetupTexts }).lazy(() =>
	import('../routes/welcome.lazy').then((module) => module.WelcomeRoute),
);

const overview = createRoute({ getParentRoute: () => app, path: '/' }).lazy(() =>
	import('../routes/overview.lazy').then((module) => module.OverviewRoute),
);

const history = createRoute({ getParentRoute: () => app, path: '/history', validateSearch: parseHistorySearch }).lazy(
	() => import('../routes/history.lazy').then((module) => module.HistoryRoute),
);

const singleImages = createRoute({
	getParentRoute: () => app,
	path: '/single-images',
	validateSearch: parseHistorySearch,
}).lazy(() => import('../routes/history.lazy').then((module) => module.SingleImagesRoute));

const newFolder = createRoute({ getParentRoute: () => app, path: '/folders/new', loader: loadSetupTexts }).lazy(() =>
	import('../routes/new-folder.lazy').then((module) => module.NewFolderRoute),
);

const folder = createRoute({ getParentRoute: () => app, path: '/folders/$folderId' }).lazy(() =>
	import('../routes/folder.lazy').then((module) => module.FolderSeriesRoute),
);

const folderSettings = createRoute({ getParentRoute: () => app, path: '/folders/$folderId/settings' }).lazy(() =>
	import('../routes/folder.lazy').then((module) => module.FolderSettingsRoute),
);

const series = createRoute({ getParentRoute: () => app, path: '/series/$seriesId' }).lazy(() =>
	import('../routes/series.lazy').then((module) => module.SeriesRoute),
);

const settings = createRoute({ getParentRoute: () => app, path: '/settings/$section', loader: loadSetupTexts }).lazy(
	() => import('../routes/settings.lazy').then((module) => module.SettingsRoute),
);

const tree = root.addChildren([
	welcome,
	app.addChildren([overview, history, singleImages, newFolder, folder, folderSettings, series, settings]),
]);

export const router = createRouter({ routeTree: tree, defaultPreload: false });

declare module '@tanstack/react-router' {
	interface Register {
		router: typeof router;
	}
}
