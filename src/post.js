import * as core from "@actions/core";
import { DefaultArtifactClient } from "@actions/artifact";
import * as exec from "@actions/exec";
import * as fs from "node:fs";
import * as path from "node:path";

async function run() {
  try {
    // post-if: failure() in action.yml guarantees we only run when the job
    // has failed — no API calls or extra permissions needed.
    core.error("[rewindr] A failure has been detected in the CI environment!");
    core.info("[rewindr] Starting state freeze sequence...");

    const workspace = process.env.GITHUB_WORKSPACE || process.cwd();
    const dumpDir = path.join(workspace, "rewindr_dump");
    if (!fs.existsSync(dumpDir)) {
      fs.mkdirSync(dumpDir);
    }

    // 1. Dump environment variables
    core.info("[rewindr] Dumping environment variables...");
    const envData = Object.entries(process.env)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n");
    fs.writeFileSync(path.join(dumpDir, "env_dump.txt"), envData);

    // 2. Dump GitHub context
    core.info("[rewindr] Dumping GitHub context...");
    const context = {
      runId: process.env.GITHUB_RUN_ID,
      runNumber: process.env.GITHUB_RUN_NUMBER,
      job: process.env.GITHUB_JOB,
      workflow: process.env.GITHUB_WORKFLOW,
      actor: process.env.GITHUB_ACTOR,
      repository: process.env.GITHUB_REPOSITORY,
      ref: process.env.GITHUB_REF,
      sha: process.env.GITHUB_SHA,
      eventName: process.env.GITHUB_EVENT_NAME,
    };
    fs.writeFileSync(
      path.join(dumpDir, "github_context.json"),
      JSON.stringify(context, null, 2),
    );

    // 3. Compress workspace (excluding rewindr_dump to avoid recursion)
    core.info("[rewindr] Compressing workspace...");
    const tarPath = path.join(dumpDir, "workspace_dump.tar.gz");
    await exec.exec("tar", [
      "-czf",
      tarPath,
      "--exclude=rewindr_dump",
      "-C",
      workspace,
      ".",
    ]);

    // 4. Upload artifact
    core.info("[rewindr] Uploading state artifact...");
    const artifactClient = new DefaultArtifactClient();
    const runId = process.env.GITHUB_RUN_ID;
    const job = process.env.GITHUB_JOB;
    const artifactName = `rewindr-state-${runId}-${job}`;

    await artifactClient.uploadArtifact(
      artifactName,
      [
        path.join(dumpDir, "env_dump.txt"),
        path.join(dumpDir, "github_context.json"),
        tarPath,
      ],
      dumpDir,
    );

    core.info(`[rewindr] State artifact uploaded: ${artifactName}`);
  } catch (error) {
    core.warning(
      `[rewindr] Error during post-execution phase: ${error.message}`,
    );
  }
}
run();
