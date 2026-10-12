import { execFileSync } from 'node:child_process';

const repo = process.env.GITHUB_REPOSITORY ?? '';
const runId = process.env.GITHUB_RUN_ID ?? '';
const runUrl = `${process.env.GITHUB_SERVER_URL}/${repo}/actions/runs/${runId}`;
const LABEL = 'nightly-red';
const RED = new Set(['failure', 'timed_out', 'cancelled']);

const gh = (args) => execFileSync('gh', args, { encoding: 'utf8' });
const json = (args) => JSON.parse(gh(args));

const jobs = json(['api', '--paginate', '--slurp', `repos/${repo}/actions/runs/${runId}/jobs?per_page=100`])
	.flatMap((page) => page.jobs)
	.filter((job) => job.status === 'completed' && job.name !== 'Nightly report');
const open = json([
	'issue',
	'list',
	'--repo',
	repo,
	'--label',
	LABEL,
	'--state',
	'open',
	'--limit',
	'200',
	'--json',
	'number,title',
]);

gh(['label', 'create', LABEL, '--repo', repo, '--color', 'B60205', '--force']);
for (const job of jobs) {
	const title = `${LABEL}: ${job.name}`;
	const issue = open.find((candidate) => candidate.title === title);
	if (RED.has(job.conclusion) && issue === undefined) {
		gh([
			'issue',
			'create',
			'--repo',
			repo,
			'--label',
			LABEL,
			'--title',
			title,
			'--body',
			`${job.conclusion} in ${runUrl}`,
		]);
	} else if (RED.has(job.conclusion)) {
		gh(['issue', 'comment', String(issue.number), '--repo', repo, '--body', `still ${job.conclusion} in ${runUrl}`]);
	} else if (issue !== undefined && job.conclusion === 'success') {
		gh(['issue', 'close', String(issue.number), '--repo', repo, '--comment', `green again in ${runUrl}`]);
	}
	process.stdout.write(`nightly-report: ${job.name}: ${job.conclusion}\n`);
}
