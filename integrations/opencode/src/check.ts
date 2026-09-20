/**
 * The two decisions this plugin makes, as plain functions.
 *
 * Nothing here imports the harness. That is deliberate: these are the
 * parts worth testing, and they can be tested anywhere, including where
 * OpenCode is not installed. The module that talks to the harness carries
 * no decision of its own.
 *
 * The types below are a minimal, local shape for the fields this file
 * reads. OpenCode does publish `@opencode-ai/plugin` and `@opencode-ai/sdk`
 * with richer types for a session message, but importing them here would
 * tie this file to the harness package again. The index signature on
 * `ConversationPart` lets a real message carry many more fields than the
 * two this file reads.
 */

export interface ConversationPart {
  readonly type: string;
  readonly text?: string;
  readonly [key: string]: unknown;
}

export interface ConversationEntry {
  readonly info: { readonly role: string };
  readonly parts?: ReadonlyArray<ConversationPart>;
}

/**
 * The text of the last assistant message in a session, or `""` when
 * there is none.
 *
 * `entries` is what OpenCode's `session.messages` call returns: a list of
 * `{ info, parts }` pairs, oldest first. Only the text parts of an
 * assistant message are the reply: a tool call or a step marker is not
 * written for a reader, so checking either would report on text no reader
 * sees.
 *
 * A message holding only non-text parts is passed over rather than
 * treated as an empty reply, so a turn that ended in a tool call still
 * gets its last real reply checked.
 */
export function lastAssistantText(entries?: ReadonlyArray<ConversationEntry>): string {
  const list = entries ?? [];
  for (let i = list.length - 1; i >= 0; i -= 1) {
    const entry = list[i];
    if (entry === undefined) continue;
    if (entry.info.role !== "assistant") continue;
    const text = (entry.parts ?? [])
      .filter(
        (part): part is ConversationPart & { text: string } =>
          part.type === "text" && typeof part.text === "string",
      )
      .map((part) => part.text)
      .join("\n")
      .trim();
    if (text !== "") return text;
  }
  return "";
}

/** What `runCheck` in `./index.ts` resolves to: either it could not run, or it ran to some exit code. */
export type CheckOutcome =
  | { readonly error: string }
  | { readonly code: number; readonly stdout: string };

/** What the check's exit meant. */
export type CheckResult =
  | { readonly kind: "passed" }
  | { readonly kind: "refused"; readonly reason: string }
  | { readonly kind: "did-not-run"; readonly detail: string };

function isBlockDecision(
  value: unknown,
): value is { readonly decision: "block"; readonly reason: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    "decision" in value &&
    "reason" in value &&
    value.decision === "block" &&
    typeof value.reason === "string" &&
    value.reason !== ""
  );
}

/**
 * `osf hook stop --answer decision-json` prints `{"decision":"block",...}`
 * on standard output and exits 0 when it refuses; it exits 0 with no
 * output when it passes. Anything else is the check failing to run, and
 * is reported as such.
 *
 * That case is the reason this function exists: a check that could not
 * run looks exactly like a check that ran and found nothing, and reading
 * the first as the second is how a gate goes quiet without anyone
 * noticing. It matters even more here: this plugin cannot block a turn,
 * so a silent "did not run" would leave a bad reply looking clean.
 */
export function readResult(outcome: CheckOutcome): CheckResult {
  if ("error" in outcome) return { kind: "did-not-run", detail: outcome.error };
  if (outcome.code !== 0) {
    return { kind: "did-not-run", detail: `exit code ${outcome.code}` };
  }

  const trimmed = outcome.stdout.trim();
  if (trimmed === "") return { kind: "passed" };

  let decision: unknown;
  try {
    decision = JSON.parse(trimmed);
  } catch (e: unknown) {
    const message = e instanceof Error ? e.message : String(e);
    return { kind: "did-not-run", detail: `could not parse the answer: ${message}` };
  }

  if (isBlockDecision(decision)) {
    return { kind: "refused", reason: decision.reason };
  }
  return { kind: "did-not-run", detail: `unexpected answer: ${trimmed}` };
}
