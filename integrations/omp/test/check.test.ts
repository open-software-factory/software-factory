/**
 * Tests for the two decisions this hook makes: which text to check, and
 * what the check's answer meant.
 *
 * Both are pure functions taking data and returning data, so neither test
 * starts a process or touches omp. The part that does start a process is
 * thin on purpose and carries no decision of its own.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { lastAssistantText, readResult, type ContentBlock, type Message } from "../src/check.js";

const assistant = (...blocks: readonly ContentBlock[]): Message => ({
  role: "assistant",
  content: blocks,
});
const user = (): Message => ({ role: "user" });
const text = (t: string): ContentBlock => ({ type: "text", text: t });

test("the direct last assistant message is checked", () => {
  assert.equal(lastAssistantText(assistant(text("the reply")), []), "the reply");
});

test("a user message is never checked", () => {
  assert.equal(lastAssistantText(user(), []), "");
});

test("several text blocks in one message are joined", () => {
  assert.equal(lastAssistantText(assistant(text("one"), text("two")), []), "one\ntwo");
});

test("only the text blocks are checked", () => {
  const message = assistant({ type: "reasoning" }, { type: "tool-call" }, text("the reply"));
  assert.equal(lastAssistantText(message, []), "the reply");
});

test("a message with no text at all gives nothing to check when the list is empty too", () => {
  assert.equal(lastAssistantText(assistant({ type: "tool-call" }), []), "");
  assert.equal(lastAssistantText(undefined, []), "");
});

test("a tool-only direct message falls back to the last real reply in the list", () => {
  const messages = [assistant(text("the reply")), assistant({ type: "tool-call" })];
  const direct = assistant({ type: "tool-call" });
  assert.equal(lastAssistantText(direct, messages), "the reply");
});

test("a refusal carries its reason", () => {
  const result = readResult({
    code: 0,
    stdout: '{"decision":"block","reason":"message:1: error [bare-reference]"}',
  });
  assert.deepEqual(result, { kind: "refused", reason: "message:1: error [bare-reference]" });
});

test("a clean check passes", () => {
  assert.deepEqual(readResult({ code: 0, stdout: "" }), { kind: "passed" });
  assert.deepEqual(readResult({ code: 0, stdout: "   \n" }), { kind: "passed" });
});

/// The one that matters. Every way of failing to run is reported as
/// such, never as a pass: a check that could not run looks exactly like
/// a check that found nothing, and the difference is the whole point.
test("a check that could not run is never read as a pass", () => {
  for (const input of [
    { error: "cannot start osf: not found" },
    { code: 127, stdout: "" },
    { code: 1, stdout: "" },
    { code: 0, stdout: "not json" },
    { code: 0, stdout: '{"decision":"block"}' },
    { code: 0, stdout: '{"decision":"allow","reason":"fine"}' },
  ]) {
    assert.equal(readResult(input).kind, "did-not-run", JSON.stringify(input));
  }
});

test("a failure to run says what went wrong", () => {
  const missingBinary = readResult({ code: 127, stdout: "" });
  assert.equal(missingBinary.kind, "did-not-run");
  assert.match(missingBinary.kind === "did-not-run" ? missingBinary.detail : "", /127/u);

  const timedOut = readResult({ error: "no answer within 10000ms" });
  assert.equal(timedOut.kind, "did-not-run");
  assert.match(timedOut.kind === "did-not-run" ? timedOut.detail : "", /10000ms/u);
});
