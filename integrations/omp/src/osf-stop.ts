/**
 * Runs the osf writing check when a main-agent turn in omp is about to
 * settle.
 *
 * omp is a coding agent, a fork of the pi coding agent project. A file
 * placed in its global hooks directory is loaded as an extension and can
 * subscribe to the `session_stop` event, which fires once per turn before
 * the session settles and can ask omp for one continuation turn by
 * returning a block decision.
 *
 * Every decision lives in `./check.js`, which imports nothing from omp.
 * What is left here is the wiring: read the reply out of the event, hand
 * it to `osf hook stop`, and turn the answer into the shape omp expects
 * back.
 */
import { spawn } from "node:child_process";
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent/extensibility/extensions";
import { lastAssistantText, readResult, type RunOutcome } from "./check.js";

const OSF_COMMAND = process.env["OSF_COMMAND"] ?? "osf";
const DEFAULT_TIMEOUT_MS = 10_000;
const HOOK_NAME = "osf-writing-check";

/** Sends `payload` to `osf hook stop --answer decision-json` and reports how it exited. */
function runCheck(payload: Record<string, unknown>): Promise<RunOutcome> {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(OSF_COMMAND, ["hook", "stop", "--answer", "decision-json"], {
        stdio: ["pipe", "pipe", "ignore"],
      });
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      resolve({ error: `cannot start ${OSF_COMMAND}: ${message}` });
      return;
    }

    let stdout = "";
    let settled = false;
    const finish = (result: RunOutcome): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };

    const timer = setTimeout(() => {
      child.kill();
      finish({ error: `no answer within ${DEFAULT_TIMEOUT_MS}ms` });
    }, DEFAULT_TIMEOUT_MS);

    child.stdout?.on("data", (chunk: Buffer | string) => {
      stdout += chunk;
    });
    child.on("error", (e) => finish({ error: `cannot run ${OSF_COMMAND}: ${e.message}` }));
    child.on("close", (code) => finish(code === null ? { stdout } : { code, stdout }));

    // Writing to a process that has already gone gives EPIPE. That is a
    // failure to run, not a reason to take omp down with it.
    child.stdin?.on("error", (e) => finish({ error: `cannot send the message: ${e.message}` }));
    child.stdin?.end(JSON.stringify(payload));
  });
}

export default function (pi: ExtensionAPI): void {
  pi.on("session_stop", async (event) => {
    // omp caps session_stop continuations at 8 and marks the replay with
    // `stop_hook_active`. Running again on a replay it caused itself
    // would waste the budget on a message this hook already checked.
    if (event.stop_hook_active) return;

    const text = lastAssistantText(event.last_assistant_message, event.messages);
    if (text === "") {
      // Nothing was said, so there is nothing to check. This is the one
      // case where silence is right, and it is narrow on purpose: every
      // other way of ending up without a result is reported below.
      return;
    }

    const result = readResult(
      await runCheck({
        session_id: event.session_id,
        turn_id: event.turn_id,
        hook_event_name: "Stop",
        last_assistant_message: text,
      }),
    );

    if (result.kind === "did-not-run") {
      // Loud, because a check that did not run looks exactly like a check
      // that found nothing, and the difference is the whole point.
      pi.logger.warn(
        `${HOOK_NAME}: the check did not run, so nothing was checked: ${result.detail}`,
      );
      return;
    }
    if (result.kind === "passed") return;

    return { decision: "block", reason: result.reason };
  });
}
