<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/rewindr-logo-light.svg" />
    <img src="assets/rewindr-logo-black.svg" alt="rewindr" width="240" />
  </picture>
</p>

<p align="center">
  <em>Rebuild and debug a failed CI run on your machine.</em>
</p>

---

## Why

When a CI job fails, the logs show you what broke but not the state it broke in.
You can re-run the job with an interactive SSH session (tmate and similar), but
that means re-triggering the run and debugging a live runner that's time-limited
and gone the moment the job ends.

rewindr takes the other approach: when a job fails it captures the environment
(your files, the environment variables, a matching runner image) into an
artifact, and the CLI rebuilds it locally. You get a shell in that environment,
on your machine, for as long as you need, without re-running anything.

## How it works

1. **The action** runs as a `post` step in your workflow. When the job fails it
   captures the workspace, the environment, and a small manifest describing the
   run, and uploads them as one artifact.
2. **The CLI** pulls that artifact, rebuilds the environment in Docker on a base
   image that mirrors the runner, mounts your code where the run expected it, and
   shells you in.

## Quick start

### 1. Add the action to your workflow

```yaml
- uses: actions/checkout@v4
- uses: dr-alberto/rewindr@v1
  with:
    # Lets rewindr redact secret values from the captured environment.
    secrets: ${{ toJSON(secrets) }}
# ... the rest of your job ...
```

It only does work when the job fails, so green runs are
untouched. This repo runs rewindr on its own CI if you want a working example:
[.github/workflows/test-action.yml](.github/workflows/test-action.yml).

### 2. Install the CLI

```bash
git clone https://github.com/dr-alberto/rewindr
cargo install --path rewindr/cli
```

### 3. Authenticate

```bash
rewindr login
```

You'll need a GitHub token with read access to Actions (fine-grained: _Actions:
read_; classic: `repo`). It's stored locally, readable only by you.

### 4. Rewind a failure

```bash
rewindr list                 # failed runs that have a rewindr artifact
rewindr play latest          # rebuild the most recent one and shell in
rewindr play 1234567890      # or a specific run id
```

You land in a shell, inside the failed environment, at the workspace.

## Commands

| Command                             | What it does                                                       |
| ----------------------------------- | ------------------------------------------------------------------ |
| `rewindr login`                     | Store a GitHub token.                                              |
| `rewindr list`                      | List runs that have a rewindr artifact.                            |
| `rewindr download <run_id\|latest>` | Pull an artifact into the local cache, like `docker pull`.         |
| `rewindr play <run_id\|latest>`     | Rebuild the environment and shell in, downloading first if needed. |

Useful `play` flags:

- `--build-only`: prepare everything and print the `docker` command without entering.
- `--image <ref>`: override the base image.
- `--repo <owner/repo>`: target a repo other than the one you're in.
- `--dir <path>`: play an already-extracted artifact directory.

Downloaded artifacts are cached globally (on Linux, under
`~/.local/share/rewindr/`), so you download once and play from anywhere.

## Fidelity

rewindr aims to drop you into something as close to the real runner as it can.
This is a work in progress; here's where it stands today.

- [x] The workspace at the moment of failure, mounted at the run's original `GITHUB_WORKSPACE` path
- [x] The run's environment variables (secrets removed, see below)
- [x] A base image that mirrors the GitHub runner, via the [catthehacker](https://github.com/catthehacker/docker_images) images (the ones [`act`](https://github.com/nektos/act) uses)
- [ ] Tools the workflow installs at runtime (`apt-get install`, `npm i -g`, and so on)
- [ ] Service containers and external state (databases, third-party APIs)
- [ ] Jobs that ran on Windows or macOS runners (rewindr rebuilds inside a Linux container)

For anything not restored yet, the captured manifest records the workflow file,
so you can see exactly what the run did and replay those steps inside the shell.

## Secrets

rewindr redacts secret values from captured environment variables when you
pass `secrets: ${{ toJSON(secrets) }}` to the action. Full details on what's
covered and what isn't: [SECURITY.md](SECURITY.md)

## Requirements

Docker (running) and `tar`. The action needs no permissions beyond uploading
artifacts.

## License

See [LICENSE](LICENSE).
