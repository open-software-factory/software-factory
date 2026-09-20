/**
 * Tests for the two decisions this plugin makes: which text to check, and
 * what the check's answer meant.
 *
 * Both are pure functions taking data and returning data, so neither test
 * starts a process or touches a network. The part that does start a
 * process is thin on purpose and carries no decision of its own.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { lastAssistantText, readResult } from "../src/check.js";
import type { CheckResult, SessionMessage, SessionMessageBlock } from "../src/check.js";

/** Narrows a result to the `did-not-run` case, failing the test otherwise. */
function assertDidNotRun(
  result: CheckResult,
): asserts result is Extract<CheckResult, { kind: "did-not-run" }> {
  assert.equal(result.kind, "did-not-run");
}

/// dsh reads a plugin only from an `insert` entry that carries an `id` and
/// the package `name`. A bare `- name:` row is accepted by the installer
/// and then silently never loaded, which is how the first version of this
/// file shipped. This pins the shape without needing a YAML parser.
test("the bundle patch registers the plugin with an insert entry", () => {
  // Resolved from the compiled test's location, two levels under the
  // package root where these YAML files live.
  for (const file of ["../../cordis.patch.yml", "../../profile-patch.example.yml"]) {
    const text = readFileSync(fileURLToPath(new URL(file, import.meta.url)), "utf8");
    const lines = text.split("\n").filter((l) => !l.trim().startsWith("#"));
    const body = lines.join("\n");
    assert.match(body, /^- insert:\s*$/mu, `${file}: no insert entry`);
    assert.match(body, /^\s+- id: \S+/mu, `${file}: insert entry has no id`);
    assert.match(
      body,
      /^\s+name: '@open-software-factory\/osf-dsh-plugin'/mu,
      `${file}: plugin not named`,
    );
    assert.doesNotMatch(body, /^- name:/mu, `${file}: a bare name row is not read by dsh`);
  }
});

const assistant = (...blocks: SessionMessageBlock[]): SessionMessage => ({
  role: "assistant",
  content: blocks,
});
const user = (text: string): SessionMessage => ({
  role: "user",
  content: [{ type: "text", text }],
});
const text = (t: string): SessionMessageBlock => ({ type: "text", text: t });

test("the last assistant message is the one checked", () => {
  const messages = [assistant(text("first")), user("a question"), assistant(text("second"))];
  assert.equal(lastAssistantText(messages), "second");
});

test("a user message is never checked", () => {
  assert.equal(lastAssistantText([user("only a question")]), "");
});

test("several text blocks in one message are joined", () => {
  assert.equal(lastAssistantText([assistant(text("one"), text("two"))]), "one\ntwo");
});

/// Reasoning and tool calls are not written for a reader, so checking
/// them would report on text no reader sees.
test("only the text blocks are checked", () => {
  const message = assistant(
    { type: "reasoning", text: "thinking out loud" },
    { type: "tool-call" },
    text("the reply"),
  );
  assert.equal(lastAssistantText([message]), "the reply");
});

/// A turn that said nothing has nothing to check. This is the only case
/// where silence is the right answer.
test("a message with no text at all gives nothing to check", () => {
  assert.equal(lastAssistantText([assistant({ type: "tool-call" })]), "");
  assert.equal(lastAssistantText([]), "");
});

/// A tool-only message must not hide an earlier reply that does have text.
test("an earlier reply is used when the last message is tool calls only", () => {
  const messages = [assistant(text("the reply")), assistant({ type: "tool-call" })];
  assert.equal(lastAssistantText(messages), "the reply");
});

test("a refusal carries its reason", () => {
  const result = readResult({ code: 2, stderr: "message:1: error [bare-reference]\n" });
  assert.deepEqual(result, { kind: "refused", reason: "message:1: error [bare-reference]" });
});

test("a clean check passes", () => {
  assert.deepEqual(readResult({ code: 0, stderr: "" }), { kind: "passed" });
});

/// The one that matters. Every way of failing to run is reported as
/// such, never as a pass: a check that could not run looks exactly like
/// a check that found nothing, and the difference is the whole point.
test("a check that could not run is never read as a pass", () => {
  for (const input of [
    { error: "cannot start osf: not found" },
    { code: 127, stderr: "" },
    { code: 1, stderr: "something broke" },
    { code: 2, stderr: "" },
    { code: 2, stderr: "   \n" },
  ]) {
    assert.equal(readResult(input).kind, "did-not-run", JSON.stringify(input));
  }
});

test("a failure to run says what went wrong", () => {
  const exitOnly = readResult({ code: 127, stderr: "" });
  assertDidNotRun(exitOnly);
  assert.match(exitOnly.detail, /127/u);

  const noAnswer = readResult({ error: "no answer within 10000ms" });
  assertDidNotRun(noAnswer);
  assert.match(noAnswer.detail, /10000ms/u);
});
