/**
 * Tests for the two decisions this plugin makes: which text to check, and
 * what the check's answer meant.
 *
 * Both are pure functions taking data and returning data, so neither test
 * starts a process or talks to OpenCode. The part that does start a
 * process is thin on purpose and carries no decision of its own.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  lastAssistantText,
  readResult,
  type CheckOutcome,
  type ConversationEntry,
  type ConversationPart,
} from "../src/check.js";

const entry = (role: string, ...parts: ConversationPart[]): ConversationEntry => ({
  info: { role },
  parts,
});
const text = (t: string): ConversationPart => ({ type: "text", text: t });

test("the last assistant message is the one checked", () => {
  const entries = [
    entry("assistant", text("first")),
    entry("user", text("a question")),
    entry("assistant", text("second")),
  ];
  assert.equal(lastAssistantText(entries), "second");
});

test("a user message is never checked", () => {
  assert.equal(lastAssistantText([entry("user", text("only a question"))]), "");
});

test("several text parts in one message are joined", () => {
  assert.equal(lastAssistantText([entry("assistant", text("one"), text("two"))]), "one\ntwo");
});

test("only the text parts are checked", () => {
  const message = entry(
    "assistant",
    { type: "tool", name: "read" },
    { type: "step-start" },
    text("the reply"),
  );
  assert.equal(lastAssistantText([message]), "the reply");
});

test("a message with no text at all gives nothing to check", () => {
  assert.equal(lastAssistantText([entry("assistant", { type: "tool", name: "read" })]), "");
  assert.equal(lastAssistantText([]), "");
  assert.equal(lastAssistantText(), "");
});

test("a tool-only reply falls back to an earlier real reply", () => {
  const entries = [
    entry("assistant", text("the reply")),
    entry("assistant", { type: "tool", name: "read" }),
  ];
  assert.equal(lastAssistantText(entries), "the reply");
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

/// The one that matters most here: this plugin cannot block a turn, so a
/// silent "did not run" would leave a bad reply looking clean.
test("a check that could not run is never read as a pass", () => {
  const inputs: CheckOutcome[] = [
    { error: "cannot start osf: not found" },
    { code: 127, stdout: "" },
    { code: 1, stdout: "" },
    { code: 0, stdout: "not json" },
    { code: 0, stdout: '{"decision":"block"}' },
    { code: 0, stdout: '{"decision":"allow","reason":"fine"}' },
  ];
  for (const input of inputs) {
    assert.equal(readResult(input).kind, "did-not-run", JSON.stringify(input));
  }
});

test("a failure to run says what went wrong", () => {
  const byCode = readResult({ code: 127, stdout: "" });
  const byTimeout = readResult({ error: "no answer within 10000ms" });
  assert.equal(byCode.kind, "did-not-run");
  assert.equal(byTimeout.kind, "did-not-run");
  if (byCode.kind === "did-not-run") assert.match(byCode.detail, /127/u);
  if (byTimeout.kind === "did-not-run") assert.match(byTimeout.detail, /10000ms/u);
});
