import * as core from "@actions/core";
import * as github from "@actions/github";

async function run() {
  try {
    core.info("[rewindr] Job finalized. Evaluating state of the pipeline...");

    const token = core.getInput("github-token");
    const octokit = github.getOctokit(token);
    const context = github.context;

    // 1. Query the "live" status of this run's jobs from the GitHub API
    const {
      data: { jobs },
    } = await octokit.rest.actions.listJobsForWorkflowRun({
      owner: context.repo.owner,
      repo: context.repo.repo,
      run_id: context.runId,
    });

    // 2. Find the current job (filtering by the context's internal ID)
    const currentJob = jobs.find(
      (j) => j.status === "in_progress" && j.name.includes(context.job),
    );

    if (!currentJob) {
      core.warning(
        "[rewindr] Could not determine the current job's status from the API.",
      );
      return;
    }

    // 3. Check whether any previous step failed
    const hasFailedStep = currentJob.steps.some(
      (step) => step.conclusion === "failure",
    );

    if (hasFailedStep) {
      core.error(
        "[rewindr] A failure has been detected in the CI environment!",
      );
      core.info("[rewindr] Starting state freeze sequence (Phase 1.2)...");

      // future code here
    } else {
      core.info(
        "[rewindr] The job finished successfully. No state capture required.",
      );
    }
  } catch (error) {
    core.warning(
      `[rewindr] Error during post-execution phase: ${error.message}`,
    );
  }
}
run();
