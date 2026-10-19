# Debugging a failed CI job over SSH

When a job fails only on the runner, open a shell on that runner with action-tmate. It works on the Windows runners too, which nektos/act cannot run.

1. Make a branch, never on `main`. Add this step at the end of the failing job:

   ```yaml
         - if: failure()
           uses: mxschmitt/action-tmate@35b54afac29c97fb54faba5b513f8fbd1882f113 # v3.24
           timeout-minutes: 30
           with:
             limit-access-to-actor: true
   ```

2. Run the workflow on the branch: `gh workflow run nightly.yml --ref <branch>`. Add `workflow_dispatch` to the workflow on the branch if it lacks it.
3. When the job fails, the step log prints an `ssh ...` command. Only the public SSH keys on your GitHub account can connect.
4. The shell starts in the checkout, with the build output and app logs in place. Leave with `touch continue` or by closing the session; the timeout ends it otherwise.
5. Delete the branch afterwards.

Keep the step out of `main`. The session can read every secret of the job, and it holds a runner for up to the timeout.

`limit-access-to-actor: true` is required for the same reason: without it anyone who reads the log can connect.

Jobs whose harden-runner uses `egress-policy: block` drop the connection to tmate.io. Switch them to `audit` on the branch.

On Windows the action uses MSYS2 from `C:\msys64`. If an image lacks it, add `msys2/setup-msys2` and pass `msys2-location`.

To rerun only the failed jobs of a run without SSH: `gh run rerun <run-id> --failed`.
