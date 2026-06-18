import * as core from "@actions/core";
import { DefaultArtifactClient } from "@actions/artifact";
import * as exec from "@actions/exec";
import * as fs from "node:fs";
import * as path from "node:path";

/// Bump when the artifact layout changes so the CLI can reject what it can't read.
const SCHEMA_VERSION = 1;

/// Values shorter than this can't be redacted by substring match without
/// shredding unrelated text, so we don't capture env when a secret is that short.
const MIN_SECRET_LENGTH = 4;

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

    // 1. Dump the environment — only if the caller declared their secrets so we
    //    can scrub them. Without that we capture nothing, to never leak.
    core.info("[rewindr] Dumping environment variables...");
    const secrets = secretValues();
    const envCaptured = secrets !== null;
    fs.writeFileSync(
      path.join(dumpDir, "env_dump.txt"),
      envCaptured ? renderEnv(secrets) : ENV_SKIPPED_NOTICE,
    );

    // 2. Compress the workspace (excluding our own dump to avoid recursion).
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

    // 3. Write the manifest the CLI reads to rebuild the environment.
    core.info("[rewindr] Writing manifest...");
    const manifest = buildManifest(envCaptured);
    fs.writeFileSync(
      path.join(dumpDir, "rewindr.json"),
      JSON.stringify(manifest, null, 2),
    );

    // 4. Upload everything as a single artifact.
    core.info("[rewindr] Uploading state artifact...");
    const artifactName = `rewindr-state-${manifest.runId}-${manifest.job}`;
    const artifactClient = new DefaultArtifactClient();
    await artifactClient.uploadArtifact(
      artifactName,
      [
        path.join(dumpDir, "rewindr.json"),
        path.join(dumpDir, "env_dump.txt"),
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

const ENV_SKIPPED_NOTICE =
  "# Environment variables were not captured.\n" +
  "# Add `secrets: ${{ toJSON(secrets) }}` to the rewindr action to capture\n" +
  "# them with all secret values redacted.\n";

/// The set of secret strings to scrub, taken from the `secrets` input
/// (`${{ toJSON(secrets) }}`). Returns null when the caller didn't declare
/// secrets, which means "do not capture the environment at all".
function secretValues() {
  const raw = core.getInput("secrets").trim();
  if (!raw) {
    return null;
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    core.warning(
      "[rewindr] Could not parse the `secrets` input; skipping env capture to stay safe.",
    );
    return null;
  }
  // Longest first, so a secret that contains another is redacted whole.
  return Object.values(parsed)
    .filter((v) => typeof v === "string" && v.length >= MIN_SECRET_LENGTH)
    .sort((a, b) => b.length - a.length);
}

/// `KEY=VALUE` lines for every env var, with secret values redacted. Drops our
/// own `INPUT_*` vars — they hold the raw secrets/token and aren't part of the
/// run environment we're reproducing.
function renderEnv(secrets) {
  return Object.entries(process.env)
    .filter(([key]) => !key.startsWith("INPUT_"))
    .map(([key, value]) => `${key}=${redact(value, secrets)}`)
    .join("\n");
}

function redact(value, secrets) {
  let out = value;
  for (const secret of secrets) {
    if (out.includes(secret)) {
      out = out.split(secret).join("***");
    }
  }
  return out;
}

function buildManifest(envCaptured) {
  const env = process.env;
  return {
    schemaVersion: SCHEMA_VERSION,
    createdAt: new Date().toISOString(),
    envCaptured,
    secretsRedacted: envCaptured,
    repository: env.GITHUB_REPOSITORY,
    sha: env.GITHUB_SHA,
    ref: env.GITHUB_REF,
    workflow: env.GITHUB_WORKFLOW,
    workflowPath: workflowPath(env),
    job: env.GITHUB_JOB,
    runId: env.GITHUB_RUN_ID,
    runNumber: env.GITHUB_RUN_NUMBER,
    actor: env.GITHUB_ACTOR,
    eventName: env.GITHUB_EVENT_NAME,
    // The original workspace path lets the CLI mount the code where the run
    // expected it, so absolute paths and $GITHUB_WORKSPACE keep resolving.
    workspacePath: env.GITHUB_WORKSPACE,
    runner: {
      os: env.RUNNER_OS,
      arch: env.RUNNER_ARCH,
      imageOS: env.ImageOS,
      imageVersion: env.ImageVersion,
    },
  };
}

/// GITHUB_WORKFLOW_REF looks like "owner/repo/.github/workflows/ci.yml@ref";
/// strip the repo prefix and the trailing git ref. The file itself lives in the
/// workspace tarball — this just points at it.
function workflowPath(env) {
  const ref = env.GITHUB_WORKFLOW_REF;
  if (!ref) {
    return null;
  }
  const withoutRef = ref.split("@")[0];
  const prefix = `${env.GITHUB_REPOSITORY}/`;
  return withoutRef.startsWith(prefix)
    ? withoutRef.slice(prefix.length)
    : withoutRef;
}

run();
