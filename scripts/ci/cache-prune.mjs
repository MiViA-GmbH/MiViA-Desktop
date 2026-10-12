import { execFileSync } from 'node:child_process';

const repo = process.env.GITHUB_REPOSITORY ?? '';
const gh = (args) => execFileSync('gh', args, { encoding: 'utf8' });

const caches = JSON.parse(
	gh(['cache', 'list', '--repo', repo, '--ref', 'refs/heads/main', '--limit', '1000', '--json', 'id,key,createdAt']),
);
const prefix = (key) => key.replace(/(-[0-9a-f]{8,})+$/, '');

const newest = new Map();
for (const cache of caches) {
	const kept = newest.get(prefix(cache.key));
	if (kept === undefined || cache.createdAt > kept.createdAt) {
		newest.set(prefix(cache.key), cache);
	}
}
const keep = new Set([...newest.values()].map((cache) => cache.id));
for (const cache of caches.filter((cache) => !keep.has(cache.id))) {
	gh(['cache', 'delete', String(cache.id), '--repo', repo]);
	process.stdout.write(`cache-prune: deleted ${cache.key} from ${cache.createdAt}\n`);
}
process.stdout.write(`cache-prune: kept ${keep.size} of ${caches.length} caches on main\n`);
