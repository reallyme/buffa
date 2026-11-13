# ReallyMe Buffa releases

ReallyMe Technologies Limited publishes all nine packages at one independent version. The registry package names, Rust aliases and exact upstream base are recorded in [FORK.md](FORK.md). The original upstream README and release history remain in [README.upstream.md](README.upstream.md) and `CHANGELOG.md`.

## One-time setup

1. Put the reviewed fork release and workflows onto the default branch of `reallyme/buffa`. GitHub requires the manual workflow to exist there before it can be dispatched on `main`. Preserve the separate upstream PR branches.
2. Create the GitHub Actions environment **`crates-io`**, configure its required reviewers, and allow deployments from `main`. Naming an environment in YAML does not configure its approval rules.
3. Create a short-lived crates.io token under the publishing account with permission to create and publish the ReallyMe packages. Store it as the GitHub Actions secret **`CARGO_REGISTRY_TOKEN`**, at repository scope or in `crates-io`. Never commit or paste it into a release command or issue.
4. Verify that the account's crates.io email is confirmed. After initial publication, add the approved ReallyMe GitHub team as an owner of every package. A team owner uses its GitHub organization/team identifier; the legal maintainer name remains ReallyMe Technologies Limited.

Repository configuration and credential setup are separate from these local files. The workflow does not add owners or configure environment protections automatically.

## Prepare and publish

Use the pinned Rust/Cargo 1.95.0 release tooling. Keep workspace and packaged lockfiles readable by Cargo 1.75; the generated table codec's minimum compiler is 1.77.

1. Update the workspace version, exact internal dependencies, package/install examples and fork release notes. Regenerate schemas and checked-in code. Review the complete release commit and run the applicable [contribution checks](CONTRIBUTING.md).
2. After approval, merge the release changes onto `main` in `reallyme/buffa`. Push only the approved source; the workflow creates the release tag after all nine crates are published.
3. Wait for the complete **CI** workflow to succeed on that exact `main` commit. Dispatch **Crates Package Preflight** on `main`, with version `0.1.0`. As in SSI, this is a separate workflow: it verifies the successful CI run and compiles all nine packaged crates without a publishing credential.
4. Inspect the successful preflight and its two artifacts: `reallyme-buffa-crates-preflight-<version>-<commit>` records the source, version and CI run; `reallyme-buffa-crate-archives-<version>-<commit>` contains the nine `.crate` files and `release.json`, binding their SHA-256 checksums and dependency order to the source commit. Both the preflight record and crate archives have GitHub build-provenance attestations. Artifacts are retained for 30 days; preserve the release record separately for long-term retention.
5. Dispatch **Crates.io Release** on the same approved `main` commit. It requires the newest matching preflight run to succeed for that version and commit, verifies its signed evidence, and waits for the `crates-io` environment approval. The upload job reproduces and compares the archives without executing Rust build scripts, and authenticates only for publication. Keep `main` at the reviewed commit until the release completes.
6. After every crate upload and checksum check succeeds, the separate finalization job creates `reallyme-v0.1.0` at that exact commit and the GitHub release **Release 0.1.0**, using the fork release notes. It attaches `release.json` as the publication record. A publication failure prevents tag and release creation.
7. Verify the published packages and add the ReallyMe team ownership. The separate **Attach protoc plugin binaries** workflow can then add downloadable generator binaries to the existing GitHub release; it cannot create a release itself.

The GitHub CLI supports an explicit preflight dispatch:

```sh
gh workflow run crates-package-preflight.yml --repo reallyme/buffa --ref main -f version=0.1.0
```

After reviewing that successful preflight, request publication separately:

```sh
gh workflow run crates-release.yml --repo reallyme/buffa --ref main
```

The release workflow publishes the crates before creating the tag and GitHub
release. Each dispatch is an explicit remote operation, separate from pushing a
branch for CI review. Neither workflow publishes automatically on branch pushes.

## Publication boundaries

- Only the `reallyme/buffa` repository can run the release jobs. Package coverage, dependency order, namespace and one shared version are checked from Cargo metadata.
- Fresh publication starts from the current `main` tip, as in ReallyMe SSI. The full CI suite must succeed for the same workflow commit. A package-only dry run cannot authorize publication by itself.
- The source, package-preflight and protected upload jobs do not persist checkout credentials. Registry credentials are absent from tests, package builds and archive comparison. The bootstrap token is exposed only to the final upload step; source checks and package commands in that step strip registry credentials, GitHub tokens and OIDC request capabilities from their child environment. Jobs that compile Rust do not have OIDC publishing permission.
- Publication uses native Cargo in dependency order. `--no-verify` is used only after the matching archives have already been built without credentials, and each package is reproduced and compared again before upload. It avoids running dependency build scripts with an upload token.
- The registry's package checksum must match the reviewed archive. A pre-existing version with different bytes, a yanked version, an altered archive, or an unavailable registry check stops publication.
- Concurrent releases of the same source commit are serialized. Transient upload rate limits and stale dependency-index errors have bounded retries; failures stop before attempting dependent packages.

## Interrupted releases

Crates.io uploads are not a transaction across the nine packages. A failed run may have published some members.

Choose **Re-run failed jobs** on the failed **Crates.io Release** run so it retains the **same source commit and
reviewed artifacts**. Keep `main` at that commit and retain the successful, first-attempt preflight run. If preflight itself fails, dispatch a fresh preflight rather than rerunning it: SSI's attestation checks require a first-attempt successful CI run and preflight. If CI needs another attempt, dispatch a fresh **CI** run (`ci.yml`) on the same `main` commit instead of rerunning the old CI run. The publisher accepts an already-published version only
when its registry checksum matches the reviewed archive, then continues in
dependency order. It never overwrites a version or skips a different archive
merely because the name/version exists.

The attestation checks require current `main` to match the release source.
Finalization uses the originally published SHA; it creates no tag or GitHub
release until publication completes. An existing release tag at a different
commit stops finalization.

## Trusted Publishing after the first release

Once the packages exist, configure a crates.io trusted publisher on **each of the nine packages**:

| Setting | Value |
| --- | --- |
| GitHub repository owner | `reallyme` |
| Repository | `buffa` |
| Workflow filename | `crates-release.yml` |
| GitHub environment | `crates-io` |

After configuring every package, remove the bootstrap secret and revoke its token. Without that secret, the workflow selects the pinned `rust-lang/crates-io-auth-action` and uses a short-lived OIDC credential. The environment approval, source checks, full CI and archive comparisons remain in place.

The optional generator-binary and tools-image workflows remain manual. The crates.io workflow creates the tag and GitHub release after successful publication, following the SSI release sequence.
