/**
 * The two decisions this hook makes, as plain functions.
 *
 * Nothing here imports the harness. That is deliberate: these are the
 * parts worth testing, and they can be tested anywhere, including where
 * the harness is not installed. The module that talks to the harness
 * carries no decision of its own.
 *
 * The types below are a minimal, self-contained shape for a message and
 * its content blocks. They are structurally compatible with omp's real
 * `AgentMessage` type, so the wiring module can pass a real event's
 * messages in without converting them first.
 */

/** A content block that carries visible text. */
export interface TextBlock {
  readonly type: "text";
  readonly text: string;
}

/** Any other content block (reasoning, a tool call, an image, ...). Its shape does not matter here. */
export interface OtherBlock {
  readonly type: string;
}

export type ContentBlock = TextBlock | OtherBlock;

/** An assistant turn: the one message shape this hook reads content from. */
export interface AssistantMessage {
  readonly role: "assistant";
  readonly content?: readonly ContentBlock[];
}

/** Any message that is not from the assistant. Its shape does not matter here. */
export interface OtherMessage {
  readonly role: string;
}

export type Message = AssistantMessage | OtherMessage;

function isTextBlock(block: ContentBlock): block is TextBlock {
  return "text" in block && typeof block.text === "string";
}

function isAssistantMessage(message: Message): message is AssistantMessage {
  return message.role === "assistant";
}

/**
 * The text of the last assistant message, or `""` when there is none.
 *
 * omp passes the stopped turn's last assistant message directly, as one
 * message with a list of content blocks. Only the text blocks are the
 * reply: reasoning and tool calls are not written for a reader, so
 * checking either would report on text no reader sees.
 *
 * When that message carries no text, the most recent assistant message
 * in the full list is used instead, so a turn that ended in a tool call
 * still gets its last real reply checked.
 */
export function lastAssistantText(
  lastAssistantMessage: Message | undefined,
  messages: readonly Message[] | undefined,
): string {
  const fromMessage = (message: Message | undefined): string => {
    if (message === undefined || !isAssistantMessage(message)) return "";
    const blocks = message.content ?? [];
    return blocks
      .filter((block): block is TextBlock => isTextBlock(block))
      .map((block) => block.text)
      .join("\n")
      .trim();
  };

  const direct = fromMessage(lastAssistantMessage);
  if (direct !== "") return direct;

  const list = messages ?? [];
  for (let i = list.length - 1; i >= 0; i -= 1) {
    const text = fromMessage(list[i]);
    if (text !== "") return text;
  }
  return "";
}

/** What running `osf hook stop --answer decision-json` produced, before it is interpreted. */
export interface RunOutcome {
  readonly code?: number;
  readonly stdout?: string;
  readonly error?: string;
}

/** What the check's exit meant. */
export type CheckResult =
  | { readonly kind: "passed" }
  | { readonly kind: "refused"; readonly reason: string }
  | { readonly kind: "did-not-run"; readonly detail: string };

/**
 * What the check's exit meant.
 *
 * `osf hook stop --answer decision-json` prints `{"decision":"block",...}`
 * on standard output and exits 0 when it refuses; it exits 0 with no
 * output when it passes. Anything else is the check failing to run, and
 * is reported as such.
 *
 * That case is the reason this function exists: a check that could not
 * run looks exactly like a check that ran and found nothing, and reading
 * the first as the second is how a gate goes quiet without anyone
 * noticing.
 */
export function readResult({ code, stdout, error }: RunOutcome): CheckResult {
  if (error) return { kind: "did-not-run", detail: error };
  if (code !== 0) {
    return { kind: "did-not-run", detail: `exit code ${code}` };
  }

  const trimmed = (stdout ?? "").trim();
  if (trimmed === "") return { kind: "passed" };

  let decision: unknown;
  try {
    decision = JSON.parse(trimmed);
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { kind: "did-not-run", detail: `could not parse the answer: ${message}` };
  }

  if (
    typeof decision === "object" &&
    decision !== null &&
    "decision" in decision &&
    decision.decision === "block" &&
    "reason" in decision &&
    typeof decision.reason === "string" &&
    decision.reason !== ""
  ) {
    return { kind: "refused", reason: decision.reason };
  }
  return { kind: "did-not-run", detail: `unexpected answer: ${trimmed}` };
}
