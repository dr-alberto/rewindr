import * as core from "@actions/core";

async function run() {
  try {
    core.info("[rewindr] Initializing state capture...");
    core.info(
      "[rewindr] Environment ready. Monitoring the rest of the pipeline.",
    );
  } catch (error) {
    core.setFailed(`rewindr Main failed: ${error.message}`);
  }
}
run();
