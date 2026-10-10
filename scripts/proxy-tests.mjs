import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

const PROXY_PY = 'proxy.py==2.4.10';
const root = resolve(import.meta.dirname, '..');
const work = join(root, 'target', 'proxy-tests');
const windows = process.platform === 'win32';

rmSync(work, { recursive: true, force: true });
mkdirSync(work, { recursive: true });

const uv = {
	...process.env,
	UV_CACHE_DIR: join(root, 'target', 'uv-cache'),
	UV_TOOL_DIR: join(root, 'target', 'uv-tools'),
	UV_PYTHON_PREFERENCE: 'only-system',
	PYTHONUNBUFFERED: '1',
};

const proxies = {
	MIVIA_PROXY_PLAIN: [],
	MIVIA_PROXY_AUTH: ['--basic-auth', 'mivia:secret'],
	MIVIA_PROXY_REFUSE: [
		'--plugins',
		'proxy.plugin.FilterByUpstreamHostPlugin',
		'--filtered-upstream-hosts',
		'localhost',
	],
};

const children = [];

const stop = () => {
	for (const child of children) {
		if (windows) {
			spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
		} else {
			child.kill();
		}
	}
};

const start = (name, extra) => {
	const portFile = join(work, `${name}.port`);
	const args = [
		'--from',
		PROXY_PY,
		'proxy',
		'--hostname',
		'127.0.0.1',
		'--port',
		'0',
		'--port-file',
		portFile,
		'--num-workers',
		'1',
		'--num-acceptors',
		'1',
		...extra,
	];
	const child = spawn('uvx', args, { env: uv, stdio: 'inherit', shell: windows });
	children.push(child);
	return portFile;
};

const waitFor = async (file) => {
	for (let attempt = 0; attempt < 600; attempt += 1) {
		if (existsSync(file)) {
			const port = readFileSync(file, 'utf8').trim();
			if (port) {
				return `127.0.0.1:${port}`;
			}
		}
		await new Promise((done) => setTimeout(done, 200));
	}
	throw new Error(`proxy.py did not start: ${file}`);
};

let status = 1;
try {
	const files = Object.entries(proxies).map(([name, extra]) => [name, start(name, extra)]);
	const env = { ...process.env };
	for (const [name, file] of files) {
		env[name] = await waitFor(file);
	}
	const result = spawnSync(
		'cargo',
		['test', '-p', 'mivia-desktop', '--test', 'proxy', '--locked', '--', '--ignored', '--test-threads=1'],
		{ env, stdio: 'inherit', shell: windows },
	);
	status = result.status ?? 1;
} finally {
	stop();
}
process.exit(status);
