/**
 * The two decisions this plugin makes, as plain functions.
 *
 * Nothing here imports the harness. That is deliberate: these are the
 * parts worth testing, and they can be tested anywhere, including where
 * OpenCode is not installed. The module that talks to the harness carries
 * no decision of its own.
 */

/**
 * The text of the last assistant message in a session, or `""` when
 * there is none.
 *
 * `entries` is what OpenCode's `session.messages` call returns: a list of
 * `{ info, parts }` pairs, oldest first. Only the text parts of an
 * assistant message are the reply: a tool call or a step marker is not
 * written for a reader, so checking either would report on text nobody
 * sees.
 *
 * A message holding only non-text parts is passed over rather than
 * treated as an empty reply, so a turn that ended in a tool call still
 * gets its last real reply checked.
 */
export function lastAssistantText(entries) {
  const list = entries ?? [];
  for (let i = list.length - 1; i >= 0; i -= 1) {
    const entry = list[i];
    if (entry?.info?.role !== "assistant") continue;
    const text = (entry.parts ?? [])
      .filter((part) => part?.type === "text" && typeof part.text === "string")
      .map((part) => part.text)
      .join("\n")
      .trim();
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
 * noticing. It matters even more here: this plugin cannot block a turn,
 * so a silent "did not run" would leave a bad reply looking clean.
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
