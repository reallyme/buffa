# ReallyMe Buffa releases

ReallyMe Technologies Limited publishes all nine packages at one independent version. The registry package names, Rust aliases and exact upstream base are recorded in [FORK.md](FORK.md). The original upstream README and release history remain in [README.upstream.md](README.upstream.md) and `CHANGELOG.md`.

## One-time setup

1. Put the reviewed fork release and workflows onto the default branch of `reallyme/buffa`. GitHub requires the manual workflow to exist there before it can be dispatched against a release tag. Preserve the separate upstream PR branches.
2. Create the GitHub Actions environment **`crates-io`**, configure its required reviewers, and allow deployments from the intended `reallyme-v*` release tags. Naming an environment in YAML does not configure its approval rules.
3. Create a short-lived crates.io token under the publishing account with permission to create and publish the ReallyMe packages. Store it as the GitHub Actions secret **`CARGO_REGISTRY_TOKEN`**, at repository scope or in `crates-io`. Never commit or paste it into a release command or issue.
4. Verify that the account's crates.io email is confirmed. After initial publication, add the approved ReallyMe GitHub team as an owner of every package. A team owner uses its GitHub organization/team identifier; the legal maintainer name remains ReallyMe Technologies Limited.

Repository configuration and credential setup are separate from these local files. The workflow does not add owners or configure environment protections automatically.

## Prepare and publish

Use the pinned Rust/Cargo 1.95.0 release tooling. Keep workspace and packaged lockfiles readable by Cargo 1.75; the generated table codec's minimum compiler is 1.77.

1. Update the workspace version, exact internal dependencies, package/install examples and fork release notes. Regenerate schemas and checked-in code. Review the complete release commit and run the applicable [contribution checks](CONTRIBUTING.md).
2. After approval, create the matching tag, for example `reallyme-v0.1.0`, pointing to that reviewed commit. Push the approved source and tag only to `reallyme/buffa`.
3. Dispatch **Publish to crates.io** against that exact tag with `dry_run=true`. This runs the complete reusable CI suite and packages every publishable crate without a registry credential. A branch dry run is also supported before creating the tag.
4. Inspect the successful run and its `reallyme-crates-<commit>` artifact. It contains the nine `.crate` files and `release.json`, which binds their SHA-256 checksums, package order and version to the source commit. Artifacts are retained for 30 days; preserve the release record separately for long-term retention.
5. Dispatch the same workflow against the approved tag with `dry_run=false`. It repeats CI and package preflight, waits for the `crates-io` environment approval, then reproduces and compares the archives without executing Rust build scripts before authenticating. The tag is rechecked immediately before publication; a moved tag or mismatched version fails closed.
6. Verify every published package and version, add the ReallyMe team ownership, and preserve the release run and checksums. Generator release binaries can then be built through the separate **Release protoc plugins** workflow for the same tag.

The GitHub CLI supports dispatching against a tag:

```sh
gh workflow run publish-crates.yml --repo reallyme/buffa --ref reallyme-v0.1.0 -f dry_run=true
```

After review, the same command with `dry_run=false` requests an actual release. A dispatch is an explicit remote operation and is separate from preparing local code or commits.

## Publication boundaries

- Only the `reallyme/buffa` repository can run the release jobs. Package coverage, dependency order, namespace and one shared version are checked from Cargo metadata.
- The full CI suite must succeed for the same workflow commit. A package-only dry run cannot authorize publication by itself.
- The source, package-preflight and protected upload jobs do not persist checkout credentials. Registry credentials are absent from tests, package builds and archive comparison. The bootstrap token is exposed only to the final upload step; source checks and package commands in that step strip registry credentials, GitHub tokens and OIDC request capabilities from their child environment. Jobs that compile Rust do not have OIDC publishing permission.
- Publication uses native Cargo in dependency order. `--no-verify` is used only after the matching archives have already been built without credentials, and each package is reproduced and compared again before upload. It avoids running dependency build scripts with an upload token.
- The registry's package checksum must match the reviewed archive. A pre-existing version with different bytes, a yanked version, an altered archive, or an unavailable registry check stops publication.
- Concurrent releases are serialized. Transient upload rate limits and stale dependency-index errors have bounded retries; failures stop before attempting dependent packages.

## Interrupted releases

Crates.io uploads are not a transaction across the nine packages. A failed run may have published some members.

Rerun the release against the **same unchanged tag and source commit**. The publisher accepts an already-published version only when the registry checksum matches the current reviewed archive, then continues in dependency order. It never overwrites a version or skips a different archive merely because the name/version exists. Do not move the tag to repair an interrupted release; investigate a mismatch before attempting further uploads.

## Trusted Publishing after the first release

Once the packages exist, configure a crates.io trusted publisher on **each of the nine packages**:

| Setting | Value |
| --- | --- |
| GitHub repository owner | `reallyme` |
| Repository | `buffa` |
| Workflow filename | `publish-crates.yml` |
| GitHub environment | `crates-io` |

After configuring every package, remove the bootstrap secret and revoke its token. Without that secret, the workflow selects the pinned `rust-lang/crates-io-auth-action` and uses a short-lived OIDC credential. The environment approval, source checks, full CI and archive comparisons remain in place.

GitHub release assets and the optional tools image have their own manual workflows. Crates.io publication does not dispatch either one automatically.
