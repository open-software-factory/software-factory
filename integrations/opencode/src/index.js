/**
 * Reports the osf writing check after every OpenCode turn.
 *
 * OpenCode is a coding agent. Its plugin `event` hook fires on bus events,
 * including `session.idle`, which lands once a session has no more work
 * queued. That hook's return type carries nothing: OpenCode cannot be
 * refused from here, unlike the dsh and omp integrations in this project.
 *
 * So this plugin only reports. It reads the session's last assistant
 * reply, sends it to `osf`, the writing check this project builds, and
 * shows a toast when the reply has a problem. `adviseOnly`, on by
 * default, is not a choice between blocking and advising: blocking is not
 * possible from this hook. It only decides how loud the report is.
 *
 * Every decision lives in `./check.js`, which imports nothing from
 * OpenCode. What is left here is the wiring.
 */
import { spawn } from "node:child_process";
import { lastAssistantText, readResult } from "./check.js";

const DEFAULT_TIMEOUT_MS = 10_000;
const PLUGIN_NAME = "osf-writing-check";

/** Sends `payload` to `osf hook stop --answer decision-json` and reports how it exited. */
function runCheck(config, payload) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(config.command, [...config.args, "hook", "stop", "--answer", "decision-json"], {
        stdio: ["pipe", "pipe", "ignore"],
      });
    } catch (e) {
      resolve({ error: `cannot start ${config.command}: ${e.message}` });
      return;
    }

    let stdout = "";
    let settled = false;
    const finish = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };

    const timer = setTimeout(() => {
      child.kill();
      finish({ error: `no answer within ${config.timeoutMs}ms` });
    }, config.timeoutMs);

    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.on("error", (e) => finish({ error: `cannot run ${config.command}: ${e.message}` }));
    child.on("close", (code) => finish({ code, stdout }));

    // Writing to a process that has already gone gives EPIPE. That is a
    // failure to run, not a reason to take the plugin down with it.
    child.stdin.on("error", (e) => finish({ error: `cannot send the message: ${e.message}` }));
    child.stdin.end(JSON.stringify(payload));
  });
}

export async function server({ client }, options) {
  const config = {
    command: process.env.OSF_COMMAND ?? "osf",
    args: [],
    timeoutMs: DEFAULT_TIMEOUT_MS,
    adviseOnly: true,
    ...options,
  };

  return {
    event: async ({ event }) => {
      if (event.type !== "session.idle") return;
      const sessionID = event.properties.sessionID;

      const history = await client.session.messages({ path: { id: sessionID } });
      if (history.error) {
        console.error(`${PLUGIN_NAME}: could not read the session, so nothing was checked: ${JSON.stringify(history.error)}`);
        return;
      }

      const text = lastAssistantText(history.data);
      if (text === "") {
        // Nothing was said, so there is nothing to check. This is the one
        // case where silence is right, and it is narrow on purpose: every
        // other way of ending up without a result is reported below.
        return;
      }

      const result = readResult(
        await runCheck(config, {
          sessionID,
          hook_event_name: "Stop",
          last_assistant_message: text,
        }),
      );

      if (result.kind === "did-not-run") {
        // Loud, because a check that did not run looks exactly like a
        // check that found nothing, and the difference is the whole
        // point. It matters even more here: this plugin cannot block a
        // turn, so a silent failure would leave a bad reply looking clean.
        console.error(`${PLUGIN_NAME}: the check did not run, so nothing was checked: ${result.detail}`);
        return;
      }
      if (result.kind === "passed") return;

      // A toast is the least intrusive surface that a viewer of the TUI
      // will actually see, and it does not add a message to the
      // conversation the way `session.prompt` would. `adviseOnly` cannot
      // turn this into a block; it only adds a second, louder report.
      await client.tui.showToast({
        body: { title: PLUGIN_NAME, message: result.reason, variant: "warning" },
      });
      if (!config.adviseOnly) {
        console.error(`${PLUGIN_NAME}: ${result.reason}`);
      }
    },
  };
}
