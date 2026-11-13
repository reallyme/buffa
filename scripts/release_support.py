#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 ReallyMe Technologies Limited
# SPDX-License-Identifier: Apache-2.0
"""Bind package preflight and publication to one immutable fork revision."""

import argparse
from dataclasses import dataclass
from enum import Enum
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib

REPOSITORY = "reallyme/buffa"
ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)")
DIGEST = re.compile(r"[0-9a-f]{64}")
COMMIT = re.compile(r"[0-9a-f]{40}")
MAX_METADATA_BYTES = 1_048_576
TRANSIENT_REGISTRY = re.compile(
    r"429|rate.limit|too many requests|no matching package|failed to select a version", re.I
)


class Reason(Enum):
    SOURCE = "release-source-invalid"
    MAIN = "release-main-changed-or-invalid"
    TAG = "release-tag-changed-or-invalid"
    PACKAGES = "release-package-set-invalid"
    ARCHIVE = "release-archive-invalid"
    CHECKSUM = "release-archive-checksum-mismatch"
    REGISTRY = "release-registry-check-failed"
    REGISTRY_RETRY = "release-registry-not-ready"
    RATE_LIMIT = "release-rate-limit-wait-exhausted"
    PUBLISH = "release-publication-failed"
    CREDENTIAL = "release-credential-missing"


class ReleaseError(Exception):
    def __init__(self, reason: Reason):
        self.reason = reason
        super().__init__(reason.value)


def without_credentials() -> dict[str, str]:
    # Builds and source checks must never inherit the upload credential.
    return {key: value for key, value in os.environ.items() if key not in {
        "CARGO_REGISTRY_TOKEN", "CARGO_REGISTRIES_CRATES_IO_TOKEN",
        "ACTIONS_ID_TOKEN_REQUEST_URL", "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
        "ACTIONS_RUNTIME_TOKEN", "GH_TOKEN", "GITHUB_TOKEN",
    }}


def command(arguments: list[str], *, registry_retry: bool = False) -> str:
    try:
        result = subprocess.run(arguments, cwd=ROOT, env=without_credentials(),
                                capture_output=True, text=True, timeout=1800,
                                check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ReleaseError(Reason.SOURCE) from error
    if result.returncode != 0:
        if registry_retry and TRANSIENT_REGISTRY.search(result.stdout + result.stderr):
            raise ReleaseError(Reason.REGISTRY_RETRY)
        raise ReleaseError(Reason.SOURCE)
    return result.stdout.strip()


@dataclass(frozen=True)
class Release:
    commit: str
    version: str
    packages: tuple[str, ...]


def select_packages(metadata: dict, ordered: list[str]) -> tuple[str, ...]:
    if not isinstance(metadata, dict):
        raise ReleaseError(Reason.PACKAGES)
    packages = metadata.get("packages")
    if (not isinstance(packages, list) or not all(isinstance(package, dict) for package in packages)
            or not ordered or not all(isinstance(name, str) for name in ordered)
            or len(set(ordered)) != len(ordered)):
        raise ReleaseError(Reason.PACKAGES)
    published = [package for package in packages if package.get("publish") != []]
    names = [package.get("name") for package in published]
    if not all(isinstance(name, str) for name in names):
        raise ReleaseError(Reason.PACKAGES)
    if set(names) != set(ordered) or len(names) != len(ordered):
        raise ReleaseError(Reason.PACKAGES)
    for package in published:
        if (not isinstance(package.get("name"), str)
                or not re.fullmatch(r"reallyme-[a-z0-9-]+", package["name"])
                or package.get("repository") != "https://github.com/reallyme/buffa"
                or package.get("license") != "Apache-2.0"
                or package.get("publish") not in (None, ["crates-io"])):
            raise ReleaseError(Reason.PACKAGES)
    if not all(isinstance(package.get("version"), str) for package in published):
        raise ReleaseError(Reason.PACKAGES)
    versions = {package["version"] for package in published}
    if len(versions) != 1 or not isinstance(next(iter(versions)), str):
        raise ReleaseError(Reason.PACKAGES)
    if VERSION.fullmatch(next(iter(versions))) is None:
        raise ReleaseError(Reason.PACKAGES)
    version = next(iter(versions))
    upstream_names = {name.removeprefix("reallyme-") for name in ordered}
    for package in published:
        dependencies = package.get("dependencies", [])
        if not isinstance(dependencies, list):
            raise ReleaseError(Reason.PACKAGES)
        for dependency in dependencies:
            if not isinstance(dependency, dict) or not isinstance(dependency.get("name"), str):
                raise ReleaseError(Reason.PACKAGES)
            if (dependency["name"] in upstream_names
                    or dependency["name"] in ordered and dependency.get("req") != "=" + version):
                raise ReleaseError(Reason.PACKAGES)
    return tuple(ordered)


def verify_identity(repository: str, sha: str, head: str, ref: str,
                    version: str, publishing: bool) -> None:
    if repository != REPOSITORY or COMMIT.fullmatch(sha) is None or sha != head:
        raise ReleaseError(Reason.SOURCE)
    if publishing and ref != "refs/heads/main":
        raise ReleaseError(Reason.MAIN)


def recorded_run_resume(environment: dict[str, str]) -> bool:
    # A human rerun preserves the original workflow SHA and reviewed artifacts.
    # A fresh dispatch from an old branch must never get this exception.
    attempt = environment.get("GITHUB_RUN_ATTEMPT", "")
    return (environment.get("GITHUB_ACTIONS") == "true"
            and environment.get("GITHUB_EVENT_NAME") == "workflow_dispatch"
            and environment.get("GITHUB_REF") == "refs/heads/main"
            and environment.get("GITHUB_WORKFLOW_REF") ==
            "reallyme/buffa/.github/workflows/crates-release.yml@refs/heads/main"
            and re.fullmatch(r"[1-9][0-9]*", attempt) is not None
            and len(attempt) <= 9 and int(attempt) >= 2)


def verify_remote_main(lines: str, commit: str, environment: dict[str, str]) -> None:
    fields = lines.split()
    if (len(fields) != 2 or fields[1] != "refs/heads/main"
            or COMMIT.fullmatch(fields[0]) is None):
        raise ReleaseError(Reason.MAIN)
    if fields[0] != commit and not recorded_run_resume(environment):
        raise ReleaseError(Reason.MAIN)


def verify_remote_tag(lines: str, ref: str, commit: str) -> None:
    entries = {}
    for line in lines.splitlines():
        fields = line.split()
        if len(fields) != 2 or fields[1] not in (ref, ref + "^{}"):
            raise ReleaseError(Reason.TAG)
        if fields[1] in entries or COMMIT.fullmatch(fields[0]) is None:
            raise ReleaseError(Reason.TAG)
        entries[fields[1]] = fields[0]
    # Annotated tags name a tag object; compare its peeled commit instead.
    if entries.get(ref + "^{}", entries.get(ref)) != commit:
        raise ReleaseError(Reason.TAG)


def source(publishing: bool = False) -> Release:
    try:
        metadata = json.loads(command(["cargo", "metadata", "--locked", "--no-deps",
                                       "--format-version", "1"]))
        ordered = command(["bash", "scripts/check-publish-coverage.sh", "--list"]).splitlines()
        command(["bash", "scripts/check-publish-coverage.sh"])
        packages = select_packages(metadata, ordered)
        version = next(package["version"] for package in metadata["packages"]
                       if package["name"] == packages[0])
        requested_version = os.environ.get("RELEASE_VERSION")
        if requested_version is not None and requested_version != version:
            raise ReleaseError(Reason.PACKAGES)
        head = command(["git", "rev-parse", "HEAD"])
        verify_identity(os.environ.get("GITHUB_REPOSITORY", ""),
                        os.environ.get("GITHUB_SHA", ""), head,
                        os.environ.get("GITHUB_REF", ""), version, publishing)
        if command(["git", "status", "--porcelain", "--untracked-files=normal"]):
            raise ReleaseError(Reason.SOURCE)
        notes = ROOT / ".changes" / "reallyme" / (version + ".md")
        if not notes.is_file():
            raise ReleaseError(Reason.SOURCE)
        if publishing:
            verify_remote_main(command(["git", "ls-remote", "--exit-code", "--heads",
                                        "origin", "refs/heads/main"]), head, dict(os.environ))
        return Release(head, version, packages)
    except (ValueError, KeyError, TypeError, StopIteration, OSError) as error:
        raise ReleaseError(Reason.SOURCE) from error


def digest(path: Path) -> str:
    try:
        with path.open("rb") as stream:
            return hashlib.file_digest(stream, "sha256").hexdigest()
    except OSError as error:
        raise ReleaseError(Reason.ARCHIVE) from error


def archive_metadata(archive: tarfile.TarFile, name: str) -> bytes:
    member = archive.getmember(name)
    if not member.isfile() or member.size > MAX_METADATA_BYTES:
        raise ReleaseError(Reason.ARCHIVE)
    stream = archive.extractfile(member)
    if stream is None:
        raise ReleaseError(Reason.ARCHIVE)
    with stream:
        return stream.read(MAX_METADATA_BYTES + 1)


def inspect_archives(release: Release, directory: Path) -> dict:
    expected = {name + "-" + release.version + ".crate" for name in release.packages}
    if {path.name for path in directory.glob("*.crate")} != expected:
        raise ReleaseError(Reason.ARCHIVE)
    packages = []
    try:
        for name in release.packages:
            prefix = name + "-" + release.version + "/"
            path = directory / (prefix[:-1] + ".crate")
            if path.is_symlink() or not path.is_file():
                raise ReleaseError(Reason.ARCHIVE)
            with tarfile.open(path) as archive:
                names = archive.getnames()
                if len(names) != len(set(names)):
                    raise ReleaseError(Reason.ARCHIVE)
                vcs = json.loads(archive_metadata(archive, prefix + ".cargo_vcs_info.json"))
                package = tomllib.loads(archive_metadata(archive, prefix + "Cargo.toml").decode())["package"]
                lock = tomllib.loads(archive_metadata(archive, prefix + "Cargo.lock").decode())
                if (vcs["git"]["sha1"] != release.commit or vcs["git"].get("dirty", False) is not False
                        or package["name"] != name or package["version"] != release.version
                        or package["repository"] != "https://github.com/reallyme/buffa"
                        or package["license"] != "Apache-2.0" or lock["version"] != 3
                        or archive_metadata(archive, prefix + "LICENSE") != (ROOT / "LICENSE").read_bytes()
                        or not archive_metadata(archive, prefix + "README.md")):
                    raise ReleaseError(Reason.ARCHIVE)
            packages.append({"name": name, "sha256": digest(path)})
    except (OSError, tarfile.TarError, ValueError, KeyError, TypeError) as error:
        raise ReleaseError(Reason.ARCHIVE) from error
    return {"schema": 1, "commit": release.commit, "version": release.version, "packages": packages}


def reviewed_manifest(release: Release, directory: Path) -> dict:
    try:
        path = directory / "release.json"
        if path.is_symlink() or path.stat().st_size > MAX_METADATA_BYTES:
            raise ReleaseError(Reason.ARCHIVE)
        document = json.loads(path.read_text())
        if (set(document) != {"schema", "commit", "version", "packages"}
                or type(document["schema"]) is not int or document["schema"] != 1
                or document["commit"] != release.commit
                or document["version"] != release.version
                or not isinstance(document["packages"], list)
                or len(document["packages"]) != len(release.packages)):
            raise ReleaseError(Reason.ARCHIVE)
        for name, package in zip(release.packages, document["packages"], strict=True):
            if (set(package) != {"name", "sha256"} or package["name"] != name
                    or not isinstance(package["sha256"], str)
                    or DIGEST.fullmatch(package["sha256"]) is None):
                raise ReleaseError(Reason.ARCHIVE)
        if inspect_archives(release, directory) != document:
            raise ReleaseError(Reason.CHECKSUM)
        return document
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise ReleaseError(Reason.ARCHIVE) from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["source", "snapshot", "verify"])
    parser.add_argument("--publishing", action="store_true")
    parser.add_argument("--directory", type=Path)
    args = parser.parse_args()
    try:
        release = source(args.publishing)
        if args.mode == "source":
            output = os.environ.get("GITHUB_OUTPUT")
            if output:
                with Path(output).open("a") as stream:
                    stream.write("commit=" + release.commit + "\nversion=" + release.version + "\n")
        elif args.directory is None:
            raise ReleaseError(Reason.ARCHIVE)
        elif args.mode == "snapshot":
            packaged = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "package"
            manifest = inspect_archives(release, packaged)
            args.directory.mkdir(parents=True, exist_ok=False)
            for name in release.packages:
                filename = name + "-" + release.version + ".crate"
                shutil.copyfile(packaged / filename, args.directory / filename)
            (args.directory / "release.json").write_text(json.dumps(manifest, indent=2) + "\n")
        else:
            expected = reviewed_manifest(release, args.directory)
            packaged = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "package"
            if inspect_archives(release, packaged) != expected:
                raise ReleaseError(Reason.CHECKSUM)
        return 0
    except ReleaseError as error:
        sys.stderr.write(error.reason.value + "\n")
        return 1
    except OSError:
        sys.stderr.write(Reason.ARCHIVE.value + "\n")
        return 1


if __name__ == "__main__":
    sys.exit(main())
