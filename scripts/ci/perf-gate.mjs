import { execFileSync } from 'node:child_process';

const repo = process.env.GITHUB_REPOSITORY ?? '';
const override = (process.env.OVERRIDE ?? '').replace(/[\r\n]/g, ' ').trim();
const PERF_JOB = /^(Performance|Release artifact smoke)/;

const gh = (args) => JSON.parse(execFileSync('gh', args, { encoding: 'utf8' }));

const blockers = gh([
	'issue',
	'list',
	'--repo',
	repo,
	'--label',
	'perf-red',
	'--state',
	'open',
	'--json',
	'number,title',
]).map((issue) => `open perf-red #${issue.number} ${issue.title}`);

const [nightly] = gh([
	'api',
	`repos/${repo}/actions/workflows/nightly.yml/runs?branch=main&status=completed&per_page=1`,
	'--jq',
	'[.workflow_runs[] | {id, html_url}]',
]);
if (nightly === undefined) {
	process.stdout.write('::warning::no completed nightly run on main, only perf-red issues were checked\n');
} else {
	const jobs = gh([
		'api',
		`repos/${repo}/actions/runs/${nightly.id}/jobs?per_page=100`,
		'--jq',
		'[.jobs[] | {name, conclusion}]',
	]);
	for (const job of jobs.filter((job) => PERF_JOB.test(job.name))) {
		process.stdout.write(`perf-gate: nightly ${job.name}: ${job.conclusion}\n`);
		if (job.conclusion !== 'success') {
			blockers.push(`nightly ${job.name} ${job.conclusion} (${nightly.html_url})`);
		}
	}
}

for (const blocker of blockers) {
	process.stdout.write(`perf-gate: ${blocker}\n`);
}
if (blockers.length === 0) {
	process.stdout.write(`perf-gate: green for ${process.env.TAG}\n`);
} else if (override === '') {
	process.stderr.write('perf-gate: performance blocks the release, pass perf_override_reason to override\n');
	process.exit(1);
} else {
	process.stdout.write(`::warning::perf gate overridden: ${override}\n`);
}
