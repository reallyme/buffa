# SPDX-FileCopyrightText: 2026 ReallyMe Technologies Limited
# SPDX-License-Identifier: Apache-2.0
"""Release boundaries are tested without credentials, uploads, or network calls."""

import io
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import urllib.error
from unittest.mock import patch

import publish_crates as publisher
import release_support as release

SHA = "a" * 40
CHECKSUM = "b" * 64
NAME = "reallyme-buffa"
VERSION = "0.1.0"
TAG = "refs/tags/reallyme-v0.1.0"
MAIN = "refs/heads/main"
RELEASE = release.Release(SHA, VERSION, (NAME,))


class SourceTests(unittest.TestCase):
    def test_upstream_readme_is_preserved(self):
        expected = "d3b4d989dddc0108e134e9365625a903346c443d21c87fb5a6c520765f1d37f8"
        self.assertEqual(hashlib.sha256((release.ROOT / "README.upstream.md").read_bytes()).hexdigest(), expected)

    def metadata(self):
        return {"packages": [{"name": NAME, "version": VERSION, "publish": None,
                              "repository": "https://github.com/reallyme/buffa",
                              "license": "Apache-2.0"}]}

    def test_package_selection(self):
        self.assertEqual(release.select_packages(self.metadata(), [NAME]), (NAME,))

    def test_rejects_wrong_package_identity_or_version(self):
        for field, value in [("name", "buffa"), ("version", "0.01.0"),
                             ("version", "0.1.0\nother"), ("license", "MIT"),
                             ("repository", "https://example.invalid/repo"),
                             ("publish", ["another-registry"])]:
            with self.subTest(field=field, value=value):
                metadata = self.metadata()
                metadata["packages"][0][field] = value
                with self.assertRaises(release.ReleaseError):
                    release.select_packages(metadata, [metadata["packages"][0]["name"]])

    def test_rejects_duplicate_missing_or_extra_packages(self):
        for ordered in [[], [NAME, NAME], [NAME, "reallyme-extra"]]:
            with self.subTest(ordered=ordered), self.assertRaises(release.ReleaseError):
                release.select_packages(self.metadata(), ordered)

    def test_rejects_mixed_versions(self):
        metadata = self.metadata()
        other = dict(metadata["packages"][0], name="reallyme-buffa-types", version="0.2.0")
        metadata["packages"].append(other)
        with self.assertRaises(release.ReleaseError):
            release.select_packages(metadata, [NAME, other["name"]])

    def test_internal_dependencies_must_select_exact_fork_versions(self):
        for name, requirement in [("buffa", "=0.1.0"), (NAME, "^0.1.0"), (NAME, "=0.2.0")]:
            metadata = self.metadata()
            metadata["packages"][0]["dependencies"] = [{"name": name, "req": requirement}]
            with self.subTest(name=name, requirement=requirement), self.assertRaises(release.ReleaseError):
                release.select_packages(metadata, [NAME])
        metadata["packages"][0]["dependencies"] = [{"name": NAME, "req": "=0.1.0"}]
        self.assertEqual(release.select_packages(metadata, [NAME]), (NAME,))

    def test_invalid_metadata_shapes_fail_closed(self):
        for metadata in [None, [], {"packages": [None]}, {"packages": [{"name": []}]},
                         {"packages": [{"name": NAME, "version": []}]}]:
            with self.subTest(metadata=metadata), self.assertRaises(release.ReleaseError):
                release.select_packages(metadata, [NAME])

    def test_source_rejects_dirty_checkout_before_publication(self):
        def respond(arguments, **_):
            if arguments[0] == "cargo":
                return json.dumps(self.metadata())
            if "--list" in arguments:
                return NAME
            if arguments[0] == "bash":
                return ""
            if "rev-parse" in arguments:
                return SHA
            if "status" in arguments:
                return " M Cargo.toml"
            self.fail("unexpected source command")
        with patch.object(release, "command", side_effect=respond), patch.dict(
                os.environ, {"GITHUB_REPOSITORY": "reallyme/buffa", "GITHUB_SHA": SHA,
                             "GITHUB_REF": MAIN}, clear=True):
            with self.assertRaises(release.ReleaseError) as error:
                release.source(publishing=True)
        self.assertEqual(error.exception.reason, release.Reason.SOURCE)

    def test_identity_requires_fork_and_exact_commit(self):
        release.verify_identity("reallyme/buffa", SHA, SHA, MAIN, VERSION, True)
        for repository, sha, head in [("another/buffa", SHA, SHA),
                                      ("reallyme/buffa", "a" * 7, SHA),
                                      ("reallyme/buffa", SHA, "c" * 40)]:
            with self.subTest(repository=repository, sha=sha), self.assertRaises(release.ReleaseError):
                release.verify_identity(repository, sha, head, MAIN, VERSION, True)

    def test_upload_requires_main_but_dry_run_accepts_release_branch(self):
        for ref in ["refs/heads/reallyme/release-0.1.0", "refs/heads/reallyme-v0.1.0", "refs/tags/v0.1.0",
                    "refs/tags/reallyme-v0.2.0"]:
            with self.subTest(ref=ref), self.assertRaises(release.ReleaseError):
                release.verify_identity("reallyme/buffa", SHA, SHA, ref, VERSION, True)
        release.verify_identity("reallyme/buffa", SHA, SHA, "refs/heads/main", VERSION, False)

    def test_current_main_is_required_for_a_fresh_publication(self):
        release.verify_remote_main(SHA + "\t" + MAIN, SHA, {})
        for lines in ["", "c" * 40 + "\t" + MAIN, SHA + "\t" + TAG,
                      SHA + "\t" + MAIN + "\n" + SHA + "\t" + MAIN]:
            with self.subTest(lines=lines), self.assertRaises(release.ReleaseError):
                release.verify_remote_main(lines, SHA, {})

    def test_only_recorded_main_workflow_reruns_can_resume_after_main_advances(self):
        environment = {
            "GITHUB_ACTIONS": "true", "GITHUB_EVENT_NAME": "workflow_dispatch",
            "GITHUB_REF": MAIN, "GITHUB_RUN_ATTEMPT": "2",
            "GITHUB_WORKFLOW_REF": "reallyme/buffa/.github/workflows/crates-release.yml@refs/heads/main",
        }
        release.verify_remote_main("c" * 40 + "\t" + MAIN, SHA, environment)
        for key, value in [("GITHUB_ACTIONS", "false"), ("GITHUB_EVENT_NAME", "push"),
                           ("GITHUB_REF", TAG), ("GITHUB_RUN_ATTEMPT", "1"),
                           ("GITHUB_RUN_ATTEMPT", "bad"), ("GITHUB_WORKFLOW_REF", "other")]:
            invalid = dict(environment, **{key: value})
            with self.subTest(key=key, value=value), self.assertRaises(release.ReleaseError):
                release.verify_remote_main("c" * 40 + "\t" + MAIN, SHA, invalid)

    def test_lightweight_and_annotated_tags(self):
        release.verify_remote_tag(SHA + "\t" + TAG, TAG, SHA)
        release.verify_remote_tag("c" * 40 + "\t" + TAG + "\n" + SHA + "\t" + TAG + "^{}", TAG, SHA)

    def test_missing_moved_ambiguous_or_unrelated_tag_is_rejected(self):
        for lines in ["", "c" * 40 + "\t" + TAG, SHA + "\trefs/heads/main",
                      SHA + "\t" + TAG + "\n" + SHA + "\t" + TAG, "invalid"]:
            with self.subTest(lines=lines), self.assertRaises(release.ReleaseError):
                release.verify_remote_tag(lines, TAG, SHA)

    def test_commands_do_not_inherit_registry_credentials(self):
        forbidden = ["CARGO_REGISTRY_TOKEN", "CARGO_REGISTRIES_CRATES_IO_TOKEN",
                     "ACTIONS_ID_TOKEN_REQUEST_TOKEN", "ACTIONS_ID_TOKEN_REQUEST_URL",
                     "ACTIONS_RUNTIME_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"]
        with patch.dict(os.environ, dict.fromkeys(forbidden, "sentinel"), clear=True):
            with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "ok")) as run:
                self.assertEqual(release.command(["git", "status"]), "ok")
            environment = run.call_args.kwargs["env"]
            for key in forbidden:
                self.assertNotIn(key, environment)

    def test_child_error_uses_fixed_diagnostic(self):
        with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "untrusted")):
            with self.assertRaises(release.ReleaseError) as error:
                release.command(["git", "status"])
        self.assertEqual(str(error.exception), "release-source-invalid")

    def test_only_explicit_registry_transients_are_retryable(self):
        for diagnostic, retryable in [("no matching package named dependency", True),
                                      ("429 Too Many Requests", True),
                                      ("permission denied", False)]:
            with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", diagnostic)):
                with self.subTest(diagnostic=diagnostic), self.assertRaises(release.ReleaseError) as error:
                    release.command(["cargo", "package"], registry_retry=True)
                self.assertEqual(error.exception.reason,
                                 release.Reason.REGISTRY_RETRY if retryable else release.Reason.SOURCE)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def write_archive(self, *, sha=SHA, dirty=False, name=NAME, lock=3,
                      license_bytes=None, duplicate=False, missing=None):
        contents = {
            ".cargo_vcs_info.json": json.dumps({"git": {"sha1": sha, "dirty": dirty}}).encode(),
            "Cargo.toml": ("[package]\nname = \"" + name + "\"\nversion = \"0.1.0\"\n"
                           "repository = \"https://github.com/reallyme/buffa\"\nlicense = \"Apache-2.0\"\n").encode(),
            "Cargo.lock": ("version = " + str(lock) + "\n").encode(),
            "LICENSE": (release.ROOT / "LICENSE").read_bytes() if license_bytes is None else license_bytes,
            "README.md": b"# Package\n",
        }
        path = self.directory / (NAME + "-0.1.0.crate")
        with tarfile.open(path, "w:gz") as archive:
            for filename, body in contents.items():
                if filename == missing:
                    continue
                member = tarfile.TarInfo(NAME + "-0.1.0/" + filename)
                member.size = len(body)
                archive.addfile(member, io.BytesIO(body))
            if duplicate:
                member = tarfile.TarInfo(NAME + "-0.1.0/README.md")
                member.size = 4
                archive.addfile(member, io.BytesIO(b"evil"))
        return path

    def test_valid_archive_snapshot_and_review(self):
        self.write_archive()
        document = release.inspect_archives(RELEASE, self.directory)
        (self.directory / "release.json").write_text(json.dumps(document))
        self.assertEqual(release.reviewed_manifest(RELEASE, self.directory), document)

    def test_rejects_dirty_wrong_source_identity_license_or_lock(self):
        for kwargs in [{"sha": "c" * 40}, {"dirty": True}, {"dirty": 0}, {"name": "buffa"},
                       {"license_bytes": b"wrong"}, {"lock": 4}, {"duplicate": True},
                       {"missing": "LICENSE"}, {"missing": "README.md"}]:
            with self.subTest(kwargs=kwargs):
                self.write_archive(**kwargs)
                with self.assertRaises(release.ReleaseError):
                    release.inspect_archives(RELEASE, self.directory)

    def test_rejects_extra_archives(self):
        self.write_archive()
        (self.directory / "unreviewed.crate").write_bytes(b"extra")
        with self.assertRaises(release.ReleaseError):
            release.inspect_archives(RELEASE, self.directory)

    def test_changed_reviewed_bytes_are_rejected(self):
        self.write_archive()
        document = release.inspect_archives(RELEASE, self.directory)
        document["packages"][0]["sha256"] = "0" * 64
        (self.directory / "release.json").write_text(json.dumps(document))
        with self.assertRaises(release.ReleaseError) as error:
            release.reviewed_manifest(RELEASE, self.directory)
        self.assertEqual(error.exception.reason, release.Reason.CHECKSUM)

    def test_rejects_malformed_or_wrong_revision_manifests(self):
        self.write_archive()
        good = release.inspect_archives(RELEASE, self.directory)
        for document in [[], {}, dict(good, schema=True), dict(good, commit="c" * 40), dict(good, packages=[]),
                         dict(good, unexpected=True), dict(good, packages=[{"name": NAME, "sha256": "bad"}])]:
            (self.directory / "release.json").write_text(json.dumps(document))
            with self.subTest(document=document), self.assertRaises(release.ReleaseError):
                release.reviewed_manifest(RELEASE, self.directory)


class PublicationTests(unittest.TestCase):
    def setUp(self):
        # A failed mock assertion must not include the developer's environment.
        environment = patch.dict(os.environ, {}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)

    def test_cli_refuses_non_ci_or_missing_credentials(self):
        for environment in [{}, {"GITHUB_ACTIONS": "true"}]:
            with patch.dict(os.environ, environment, clear=True), \
                    patch.object(sys, "argv", ["publish_crates.py", "--directory", "/not-used"]), \
                    patch.object(publisher, "source") as check, patch.object(sys, "stderr", io.StringIO()):
                self.assertEqual(publisher.main(), 1)
                check.assert_not_called()

    def test_registry_checksum_validation(self):
        body = {"version": {"crate": NAME, "num": VERSION, "checksum": CHECKSUM, "yanked": False}}
        with patch.object(publisher.urllib.request, "urlopen", return_value=io.BytesIO(json.dumps(body).encode())):
            self.assertEqual(publisher.registry_checksum(NAME, VERSION), CHECKSUM)
        for field, value in [("crate", "another"), ("num", "0.2.0"), ("checksum", "bad"), ("yanked", True)]:
            invalid = json.loads(json.dumps(body))
            invalid["version"][field] = value
            with patch.object(publisher.urllib.request, "urlopen", return_value=io.BytesIO(json.dumps(invalid).encode())):
                with self.subTest(field=field), self.assertRaises(release.ReleaseError):
                    publisher.registry_checksum(NAME, VERSION)

    def test_only_not_found_means_unpublished(self):
        for status in [404, 403, 429, 500]:
            failure = urllib.error.HTTPError("https://crates.io", status, "failure", {}, None)
            with patch.object(publisher.urllib.request, "urlopen", side_effect=failure):
                if status == 404:
                    self.assertIsNone(publisher.registry_checksum(NAME, VERSION))
                else:
                    with self.assertRaises(release.ReleaseError):
                        publisher.registry_checksum(NAME, VERSION)

    def publish(self, registry, results, packaging=None):
        with patch.object(publisher, "command", side_effect=packaging), patch.object(publisher, "digest", return_value=CHECKSUM), \
                patch.object(publisher, "source", return_value=RELEASE), \
                patch.object(publisher, "registry_checksum", side_effect=registry), \
                patch.object(publisher.subprocess, "run", side_effect=results) as upload, \
                patch.object(publisher.time, "sleep"):
            publisher.publish_one(RELEASE, NAME, CHECKSUM)
            return upload

    def test_resumes_only_matching_existing_upload(self):
        upload = self.publish([CHECKSUM], [])
        upload.assert_not_called()
        with self.assertRaises(release.ReleaseError) as error:
            self.publish(["c" * 64], [])
        self.assertEqual(error.exception.reason, release.Reason.CHECKSUM)

    def test_upload_checks_registry_bytes(self):
        result = subprocess.CompletedProcess([], 0, "", "")
        upload = self.publish([None, None, CHECKSUM], [result])
        self.assertIn("--no-verify", upload.call_args.args[0])
        self.assertIn("crates-io", upload.call_args.args[0])

    def test_retries_rate_limit_and_recognizes_upload_before_index_timeout(self):
        limited = subprocess.CompletedProcess([], 1, "", "429 Too Many Requests")
        success = subprocess.CompletedProcess([], 0, "", "")
        self.assertEqual(self.publish([None, None, None, CHECKSUM], [limited, success]).call_count, 2)
        timeout = subprocess.CompletedProcess([], 1, "", "index timeout")
        self.assertEqual(self.publish([None, CHECKSUM], [timeout]).call_count, 1)

    def test_local_mismatch_stops_before_upload(self):
        with patch.object(publisher, "command"), patch.object(publisher, "digest", return_value="c" * 64), \
                patch.object(publisher, "source", return_value=RELEASE), \
                patch.object(publisher.subprocess, "run") as upload:
            with self.assertRaises(release.ReleaseError):
                publisher.publish_one(RELEASE, NAME, CHECKSUM)
            upload.assert_not_called()

    def test_retries_transient_dependency_resolution_before_packaging(self):
        success = subprocess.CompletedProcess([], 0, "", "")
        upload = self.publish([None, CHECKSUM], [success],
                              [release.ReleaseError(release.Reason.REGISTRY_RETRY), None])
        self.assertEqual(upload.call_count, 1)

    def test_packaging_retry_exhaustion_and_permanent_errors_stop_before_upload(self):
        for reason in [release.Reason.REGISTRY_RETRY, release.Reason.SOURCE]:
            with self.subTest(reason=reason), patch.object(publisher, "command", side_effect=release.ReleaseError(reason)), \
                    patch.object(publisher, "source", return_value=RELEASE), \
                    patch.object(publisher.subprocess, "run") as upload, \
                    patch.object(publisher.time, "sleep"):
                with self.assertRaises(release.ReleaseError):
                    publisher.publish_one(RELEASE, NAME, CHECKSUM)
                upload.assert_not_called()


if __name__ == "__main__":
    unittest.main()
