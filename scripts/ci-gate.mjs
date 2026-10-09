// 校验路径过滤与实际 job 结果，防止必须执行的门禁被跳过。
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const jobs = ['changes', 'frontend', 'rust-core', 'rust-shell'];
const suites = {
    full: ['frontend', 'rust-core', 'rust-shell'],
    frontend: ['frontend'],
    rust: ['rust-core', 'rust-shell'],
    'rust-smoke': ['rust-core'],
};

export function checkGate({ event, suite = 'full', needs }) {
    const failures = [];
    const required = new Set(['changes']);
    if (event === 'pull_request' || event === 'push') {
        for (const [area, job] of [
            ['frontend', 'frontend'],
            ['rust_core', 'rust-core'],
            ['rust_shell', 'rust-shell'],
        ]) {
            const changed = needs.changes?.outputs?.[area];
            if (changed === 'true') required.add(job);
            else if (changed !== 'false') failures.push(`invalid path filter: ${area}=${changed}`);
        }
    } else if (event === 'workflow_dispatch') {
        if (!Object.hasOwn(suites, suite)) failures.push(`unknown suite: ${suite}`);
        else for (const job of suites[suite]) required.add(job);
    } else {
        failures.push(`unsupported event: ${event}`);
    }

    for (const job of jobs) {
        const result = needs[job]?.result;
        if (result === 'success' || (!required.has(job) && result === 'skipped')) continue;
        failures.push(
            `${job}=${result} (expected ${required.has(job) ? 'success' : 'success or skipped'})`,
        );
    }
    return failures;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const needs = JSON.parse(process.env.CI_JOB_RESULTS);
    const failures = checkGate({
        event: process.env.CI_EVENT,
        suite: process.env.CI_SUITE || 'full',
        needs,
    });
    console.log(Object.fromEntries(jobs.map((job) => [job, needs[job]?.result])));
    for (const failure of failures) console.error(`CI gate failed: ${failure}`);
    if (failures.length) process.exitCode = 1;
}
