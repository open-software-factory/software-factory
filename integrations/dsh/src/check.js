/**
 * The two decisions this plugin makes, as plain functions.
 *
 * Nothing here imports the harness. That is deliberate: these are the
 * parts worth testing, and they can be tested anywhere, including where
 * the harness is not installed. The module that talks to the harness
 * carries no decision of its own.
 */

/**
 * The text of the last assistant message, or `""` when there is none.
 *
 * A message is a list of blocks and only the text ones are the reply.
 * Reasoning and tool calls are skipped: neither is written for a reader,
 * so checking either would report on text nobody sees.
 *
 * A message holding only tool calls is passed over rather than treated as
 * an empty reply, so a turn that ended in a tool call still gets its last
 * real reply checked.
 */
export function lastAssistantText(messages) {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const message = messages[i];
    if (message?.role !== "assistant") continue;
    const text = (message.content ?? [])
      .filter((block) => block?.type === "text" && typeof block.text === "string")
      .map((block) => block.text)
      .join("\n")
      .trim();
    if (text !== "") return text;
  }
  return "";
}

/**
 * What the check's exit meant.
 *
 * Exit code 2 with something on standard error is the check refusing, the
 * same shape every other gate in this project uses. Exit 0 is a pass.
 *
 * Everything else is the check failing to run, and is reported as such.
 * That case is the reason this function exists: a check that could not run
 * looks exactly like a check that ran and found nothing, and reading the
 * first as the second is how a gate goes quiet without anyone noticing.
 */
export function readResult({ code, stderr, error }) {
  if (error) return { kind: "did-not-run", detail: error };
  const reason = (stderr ?? "").trim();
  if (code === 2 && reason !== "") return { kind: "refused", reason };
  if (code === 0) return { kind: "passed" };
  return {
    kind: "did-not-run",
    detail: reason === "" ? `exit code ${code}` : `exit code ${code}: ${reason}`,
  };
}
