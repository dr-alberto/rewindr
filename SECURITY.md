# Security

rewindr captures the run's environment variables so you can debug with the same
config the job had. To keep secrets out of the artifact:

- Add `secrets: ${{ toJSON(secrets) }}` to the action. rewindr reads your secret
  values and replaces every occurrence of them in the captured environment with
  `***`.
- Leave it out, and rewindr skips environment capture entirely. The run metadata
  (repo, commit, runner) is still saved, but no environment variables are, so
  nothing can leak by accident.

It also always strips the action's own inputs (`INPUT_*`) and the tokens the
runner injects (`ACTIONS_RUNTIME_TOKEN` and friends), which aren't part of your
`secrets`.

Two things it does not cover. Secrets your build writes to disk end up in the
workspace archive, which rewindr doesn't scan. And a secret shorter than four
characters is skipped rather than redacted: blanking a one or two character
string would mangle unrelated values, and GitHub's own log masking skips short
secrets for the same reason.
