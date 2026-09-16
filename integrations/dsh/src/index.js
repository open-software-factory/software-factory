/**
 * Runs the writing check at the end of every turn, inside dsh.
 *
 * dsh, the DeepSeek Harness, is one of the coding agents this project
 * supports. It ships a bridge that runs a hook file written for another
 * agent, and that bridge's end-of-turn payload carries no assistant text:
 * its `stopPayload` sends a session id, an empty transcript path, and
 * nothing else. A check reading that payload has nothing to read, so it
 * passes every turn without ever looking at a message.
 *
 * This plugin reads the message from the session instead. A plugin here
 * is an ordinary module in the same process as the harness, so the text
 * is already in memory: no file to find, no transcript to parse, nothing
 * to keep in step with an upstream format.
 *
 * It does not replace that bridge and does not conflict with it. dsh
 * delivers `agent/turn-stopping` to every listener in turn, and the
 * bridge's own listener never stops the chain, so both run when both are
 * registered. Register the bridge as well to drive the other hook events
 * from a hook file; register this alone to check the writing.
 *
 * Every decision lives in `./check.js`, which imports nothing from the
 * harness. What is left here is the wiring.
 */

import { spawn } from "node:child_process";
import z from "@deepseek-ai/schemastery";
import { createUserMessage } from "@deepseek-ai/dsh-llm";
import { lastAssistantText, readResult } from "./check.js";

/** Marks a steering message as this plugin's, not a person's. */
const PLUGIN_SOURCE = { kind: "plugin", plugin: "osf-writing-check" };

/** Long enough for a real check, short enough not to hold a turn open. */
const DEFAULT_TIMEOUT_MS = 10_000;

export const name = "osf-writing-check";

export const inject = ["logger"];

export const Config = z.object({
  /** The `osf` binary. A bare name is resolved on the path. */
  command: z.string().default("osf"),
  /** Passed before `hook stop`, for a config file or a name list. */
  args: z.array(z.string()).default([]),
  /** How long the check may take before this plugin gives up on it. */
  timeoutMs: z.number().default(DEFAULT_TIMEOUT_MS),
  /**
   * Report what the check says, but never hold the turn. For trying the
   * check out without it interrupting anyone.
   */
  adviseOnly: z.boolean().default(false),
});

/** Sends `payload` to the check and resolves with how it exited. */
function runCheck(config, payload) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(config.command, [...config.args, "hook", "stop"], {
        stdio: ["pipe", "ignore", "pipe"],
      });
    } catch (e) {
      resolve({ error: `cannot start ${config.command}: ${e.message}` });
      return;
    }

    let stderr = "";
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

    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", (e) => finish({ error: `cannot run ${config.command}: ${e.message}` }));
    child.on("close", (code) => finish({ code, stderr }));

    // Writing to a process that has already gone gives EPIPE. That is a
    // failure to run, not a reason to take the harness down with it.
    child.stdin.on("error", (e) => finish({ error: `cannot send the message: ${e.message}` }));
    child.stdin.end(JSON.stringify(payload));
  });
}

export function apply(ctx, config) {
  ctx.on("agent/turn-stopping", async ({ agent }) => {
    const session = agent?.session;
    if (!session) return;

    const text = lastAssistantText(session.deriveMessages?.() ?? []);
    if (text === "") {
      // Nothing was said, so there is nothing to check. This is the one
      // case where silence is right, and it is narrow on purpose: every
      // other way of ending up without a result is reported below.
      return;
    }

    const result = readResult(
      await runCheck(config, {
        session_id: session.header?.id ?? "",
        hook_event_name: "Stop",
        last_assistant_message: text,
      }),
    );

    if (result.kind === "did-not-run") {
      // Loud, because a check that did not run looks exactly like a check
      // that found nothing, and the difference is the whole point.
      ctx.logger?.warn?.(
        `${name}: the check did not run, so nothing was checked: ${result.detail}`,
      );
      return;
    }
    if (result.kind === "passed") return;

    if (config.adviseOnly) {
      ctx.logger?.info?.(`${name}: ${result.reason}`);
      return;
    }

    agent.steer(
      createUserMessage({
        content: [{ type: "text", text: result.reason }],
        source: PLUGIN_SOURCE,
      }),
    );
  });
}
