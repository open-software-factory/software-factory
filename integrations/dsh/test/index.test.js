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
import { lastAssistantText, readResult } from "../src/check.js";

const assistant = (...blocks) => ({ role: "assistant", content: blocks });
const user = (text) => ({ role: "user", content: [{ type: "text", text }] });
const text = (t) => ({ type: "text", text: t });

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
/// them would report on text nobody sees.
test("only the text blocks are checked", () => {
  const message = assistant(
    { type: "reasoning", text: "thinking out loud" },
    { type: "tool-call", name: "read" },
    text("the reply"),
  );
  assert.equal(lastAssistantText([message]), "the reply");
});

/// A turn that said nothing has nothing to check. This is the only case
/// where silence is the right answer.
test("a message with no text at all gives nothing to check", () => {
  assert.equal(lastAssistantText([assistant({ type: "tool-call", name: "read" })]), "");
  assert.equal(lastAssistantText([]), "");
});

/// A tool-only message must not hide an earlier reply that does have text.
test("an earlier reply is used when the last message is tool calls only", () => {
  const messages = [assistant(text("the reply")), assistant({ type: "tool-call", name: "read" })];
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
  assert.match(readResult({ code: 127, stderr: "" }).detail, /127/);
  assert.match(readResult({ error: "no answer within 10000ms" }).detail, /10000ms/);
});
