#!/usr/bin/env node
// SPDX-FileCopyrightText: Copyright © 2026 ReallyMe Technologies Limited. All rights reserved

// SPDX-License-Identifier: MIT OR Apache-2.0

import assert from "node:assert/strict";
import test from "node:test";

import {
  ReleaseAttestationError,
  selectLatestPreflightRun,
  selectLatestRustCiRun,
  verifyAttestationDocument,
  verifyRustCiRun,
  verifyWorkflowRun,
  verifyReleaseAttestation,
} from "./verify_release_attestation.mjs";

const releaseSha = "a".repeat(40);
const expected = Object.freeze({
  repository: "reallyme/buffa",
  runId: 123,
  rustCiRunId: 321,
  releaseSha,
  releaseVersion: "0.1.0",
});
const attestation = (overrides = {}) => ({
  schema: "reallyme.buffa.crates_preflight.v2",
  repository: expected.repository,
  workflow: "crates-package-preflight.yml",
  run_id: expected.runId,
  rust_ci_run_id: expected.rustCiRunId,
  run_attempt: 1,
  release_sha: releaseSha,
  version: expected.releaseVersion,
  ...overrides,
});
const workflowRun = (overrides = {}) => ({
  workflow_id: 456,
  id: expected.runId,
  event: "workflow_dispatch",
  head_branch: "main",
  head_sha: releaseSha,
  status: "completed",
  conclusion: "success",
  run_attempt: 1,
  path: ".github/workflows/crates-package-preflight.yml",
  ...overrides,
});
const listedWorkflowRun = (overrides = {}) => ({
  workflow_id: 456,
  id: expected.runId,
  event: "workflow_dispatch",
  head_branch: "main",
  head_sha: releaseSha,
  status: "completed",
  conclusion: "success",
  run_attempt: 1,
  path: ".github/workflows/crates-package-preflight.yml",
  display_title: `Crates package preflight ${expected.releaseVersion} @ ${releaseSha}`,
  ...overrides,
});
const rustCiRun = (overrides = {}) => ({
  workflow_id: 654,
  id: expected.rustCiRunId,
  event: "push",
  head_branch: "main",
  head_sha: releaseSha,
  status: "completed",
  conclusion: "success",
  run_attempt: 1,
  path: ".github/workflows/ci.yml",
  ...overrides,
});
const listedRustCiRun = (overrides = {}) => ({
  ...rustCiRun(),
  display_title: "Rust CI",
  ...overrides,
});

test("reviewed attestation accepts the exact run, SHA, and version", () => {
  assert.doesNotThrow(() => verifyAttestationDocument(attestation(), expected));
  assert.doesNotThrow(() =>
    verifyWorkflowRun(workflowRun(), {
      workflowId: 456,
      runId: expected.runId,
      releaseSha,
    }),
  );
  assert.doesNotThrow(() =>
    verifyRustCiRun(rustCiRun(), {
      workflowId: 654,
      runId: expected.rustCiRunId,
      releaseSha,
    }),
  );
});

test("attestation rejects mismatched inputs and unreviewed fields", () => {
  for (const candidate of [
    attestation({ version: "0.2.0" }),
    attestation({ release_sha: "b".repeat(40) }),
    attestation({ run_id: 124 }),
    attestation({ rust_ci_run_id: 322 }),
    attestation({ extra: true }),
  ]) {
    assert.throws(
      () => verifyAttestationDocument(candidate, expected),
      ReleaseAttestationError,
    );
  }
});

test("full Rust CI must pass on the exact release SHA", () => {
  const latest = selectLatestRustCiRun(
    {
      workflow_runs: [
        listedRustCiRun({ id: 320 }),
        listedRustCiRun(),
        listedRustCiRun({ id: 322, head_sha: "b".repeat(40) }),
      ],
    },
    { workflowId: 654, releaseSha },
  );
  assert.equal(latest.id, expected.rustCiRunId);

  for (const candidate of [
    listedRustCiRun({ id: 322, conclusion: "failure" }),
    listedRustCiRun({ id: 322, status: "in_progress", conclusion: null }),
    listedRustCiRun({ id: 322, run_attempt: 2 }),
  ]) {
    assert.throws(
      () =>
        selectLatestRustCiRun(
          { workflow_runs: [listedRustCiRun(), candidate] },
          { workflowId: 654, releaseSha },
        ),
      ReleaseAttestationError,
    );
  }
});

test("failed, rerun, wrong-branch, and wrong-workflow runs fail closed", () => {
  for (const candidate of [
    workflowRun({ conclusion: "failure" }),
    workflowRun({ run_attempt: 2 }),
    workflowRun({ head_branch: "feature" }),
    workflowRun({ workflow_id: 457 }),
    workflowRun({ path: ".github/workflows/other.yml@refs/heads/main" }),
  ]) {
    assert.throws(
      () =>
        verifyWorkflowRun(candidate, {
          workflowId: 456,
          runId: expected.runId,
          releaseSha,
        }),
      ReleaseAttestationError,
    );
  }
});

test("automatic resolution selects the latest exact successful preflight", () => {
  const latest = selectLatestPreflightRun(
    {
      workflow_runs: [
        listedWorkflowRun({ id: 121 }),
        listedWorkflowRun({ id: 123 }),
        listedWorkflowRun({ id: 124, head_sha: "b".repeat(40) }),
      ],
    },
    {
      workflowId: 456,
      releaseSha,
      releaseVersion: expected.releaseVersion,
    },
  );
  assert.equal(latest.id, 123);
});

test("newer failed, running, wrong-version, and rerun preflights fail closed", () => {
  const rejectedLatestRuns = [
    listedWorkflowRun({ id: 124, conclusion: "failure" }),
    listedWorkflowRun({ id: 124, conclusion: null, status: "in_progress" }),
    listedWorkflowRun({
      id: 124,
      display_title: `Crates package preflight 0.2.0 @ ${releaseSha}`,
    }),
    listedWorkflowRun({ id: 124, run_attempt: 2 }),
  ];
  for (const rejected of rejectedLatestRuns) {
    assert.throws(
      () =>
        selectLatestPreflightRun(
          { workflow_runs: [listedWorkflowRun({ id: 123 }), rejected] },
          {
            workflowId: 456,
            releaseSha,
            releaseVersion: expected.releaseVersion,
          },
        ),
      ReleaseAttestationError,
    );
  }
});

test("automatic resolution rejects missing and malformed workflow results", () => {
  for (const candidate of [{}, { workflow_runs: [] }, { workflow_runs: [null] }]) {
    assert.throws(
      () =>
        selectLatestPreflightRun(candidate, {
          workflowId: 456,
          releaseSha,
          releaseVersion: expected.releaseVersion,
        }),
      ReleaseAttestationError,
    );
  }
});

test("waiting rejects zero polling intervals before querying GitHub", () => {
  assert.throws(() => verifyReleaseAttestation({ env: {
    GITHUB_REPOSITORY: expected.repository,
    RELEASE_SHA: releaseSha,
    RELEASE_VERSION: expected.releaseVersion,
    GITHUB_SHA: releaseSha,
    GH_TOKEN: "test-placeholder",
    RELEASE_ATTESTATION_WAIT_SECONDS: "60",
    RELEASE_ATTESTATION_POLL_SECONDS: "0",
  } }), { name: "ReleaseAttestationError", code: "invalid-release-attestation-poll-seconds" });
});
