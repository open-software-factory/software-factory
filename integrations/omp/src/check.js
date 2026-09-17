/**
 * The two decisions this hook makes, as plain functions.
 *
 * Nothing here imports the harness. That is deliberate: these are the
 * parts worth testing, and they can be tested anywhere, including where
 * the harness is not installed. The module that talks to the harness
 * carries no decision of its own.
 */

/**
 * The text of the last assistant message, or `""` when there is none.
 *
 * omp passes the stopped turn's last assistant message directly, as one
 * message with a list of content blocks. Only the text blocks are the
 * reply: reasoning and tool calls are not written for a reader, so
 * checking either would report on text nobody sees.
 *
 * When that message carries no text, the most recent assistant message
 * in the full list is used instead, so a turn that ended in a tool call
 * still gets its last real reply checked.
 */
export function lastAssistantText(lastAssistantMessage, messages) {
  const fromMessage = (message) => {
    if (message?.role !== "assistant") return "";
    return (message.content ?? [])
      .filter((block) => block?.type === "text" && typeof block.text === "string")
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
export function readResult({ code, stdout, error }) {
  if (error) return { kind: "did-not-run", detail: error };
  if (code !== 0) {
    return { kind: "did-not-run", detail: `exit code ${code}` };
  }

  const trimmed = (stdout ?? "").trim();
  if (trimmed === "") return { kind: "passed" };

  let decision;
  try {
    decision = JSON.parse(trimmed);
  } catch (e) {
    return { kind: "did-not-run", detail: `could not parse the answer: ${e.message}` };
  }

  if (decision?.decision === "block" && typeof decision.reason === "string" && decision.reason !== "") {
    return { kind: "refused", reason: decision.reason };
  }
  return { kind: "did-not-run", detail: `unexpected answer: ${trimmed}` };
}
