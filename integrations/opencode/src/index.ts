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
 * Every decision lives in `./check.ts`, which imports nothing from
 * OpenCode. What is left here is the wiring, typed against the
 * `@opencode-ai/plugin` package OpenCode itself publishes.
 */
import { spawn, type ChildProcess } from "node:child_process";
import type { Hooks, Plugin, PluginInput, PluginOptions } from "@opencode-ai/plugin";
import { lastAssistantText, readResult, type CheckOutcome, type CheckResult } from "./check.js";

type Client = PluginInput["client"];

const DEFAULT_TIMEOUT_MS = 10_000;
const PLUGIN_NAME = "osf-writing-check";

interface PluginConfig {
  readonly command: string;
  readonly args: readonly string[];
  readonly timeoutMs: number;
  readonly adviseOnly: boolean;
}

function readStringOption(source: PluginOptions, key: string): string | undefined {
  const value = source[key];
  return typeof value === "string" ? value : undefined;
}

function readNumberOption(source: PluginOptions, key: string): number | undefined {
  const value = source[key];
  return typeof value === "number" ? value : undefined;
}

function readBooleanOption(source: PluginOptions, key: string): boolean | undefined {
  const value = source[key];
  return typeof value === "boolean" ? value : undefined;
}

function readStringArrayOption(source: PluginOptions, key: string): readonly string[] | undefined {
  const value = source[key];
  if (!Array.isArray(value)) return undefined;
  return value.every((item): item is string => typeof item === "string") ? value : undefined;
}

/** Reads plugin settings out of the untyped options OpenCode passes a plugin, falling back to this plugin's defaults. */
function readConfig(options: PluginOptions | undefined): PluginConfig {
  const source: PluginOptions = options ?? {};
  return {
    command: readStringOption(source, "command") ?? process.env["OSF_COMMAND"] ?? "osf",
    args: readStringArrayOption(source, "args") ?? [],
    timeoutMs: readNumberOption(source, "timeoutMs") ?? DEFAULT_TIMEOUT_MS,
    adviseOnly: readBooleanOption(source, "adviseOnly") ?? true,
  };
}

/** What a closed `osf` process resolves to: a clean exit, or a signal with no exit code. */
function closeOutcome(
  code: number | null,
  signal: NodeJS.Signals | null,
  stdout: string,
): CheckOutcome {
  if (code === null) {
    return {
      error: signal === null ? "terminated with no exit code" : `terminated by signal ${signal}`,
    };
  }
  return { code, stdout };
}

/** Wires up the listeners a spawned `osf` process needs, and sends it the payload. */
function wireChildEvents(
  child: ChildProcess,
  config: PluginConfig,
  payload: Record<string, unknown>,
  finish: (result: CheckOutcome) => void,
): void {
  let stdout = "";
  child.stdout?.on("data", (chunk: Buffer) => {
    stdout += chunk.toString();
  });
  child.on("error", (e: Error) => finish({ error: `cannot run ${config.command}: ${e.message}` }));
  child.on("close", (code, signal) => finish(closeOutcome(code, signal, stdout)));

  // Writing to a process that has already gone gives EPIPE. That is a
  // failure to run, not a reason to take the plugin down with it.
  child.stdin?.on("error", (e: Error) =>
    finish({ error: `cannot send the message: ${e.message}` }),
  );
  child.stdin?.end(JSON.stringify(payload));
}

/** Sends `payload` to `osf hook stop --answer decision-json` and reports how it exited. */
function runCheck(config: PluginConfig, payload: Record<string, unknown>): Promise<CheckOutcome> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (result: CheckOutcome): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };

    let child: ChildProcess;
    try {
      child = spawn(config.command, [...config.args, "hook", "stop", "--answer", "decision-json"], {
        stdio: ["pipe", "pipe", "ignore"],
      });
    } catch (e: unknown) {
      const message = e instanceof Error ? e.message : String(e);
      resolve({ error: `cannot start ${config.command}: ${message}` });
      return;
    }

    const timer = setTimeout(() => {
      child.kill();
      finish({ error: `no answer within ${config.timeoutMs}ms` });
    }, config.timeoutMs);

    wireChildEvents(child, config, payload, finish);
  });
}

/** The session's last assistant reply, or `undefined` when the session could not be read (already logged). */
async function fetchLastAssistantText(
  client: Client,
  sessionID: string,
): Promise<string | undefined> {
  const history = await client.session.messages({ path: { id: sessionID } });
  if (history.error !== undefined) {
    console.error(
      `${PLUGIN_NAME}: could not read the session, so nothing was checked: ${JSON.stringify(history.error)}`,
    );
    return undefined;
  }
  return lastAssistantText(history.data);
}

/** Shows a toast for a refusal, and logs when the check itself could not run. A pass says nothing. */
async function reportCheckResult(
  client: Client,
  config: PluginConfig,
  result: CheckResult,
): Promise<void> {
  if (result.kind === "did-not-run") {
    // Loud, because a check that did not run looks exactly like a
    // check that found nothing, and the difference is the whole
    // point. It matters even more here: this plugin cannot block a
    // turn, so a silent failure would leave a bad reply looking clean.
    console.error(
      `${PLUGIN_NAME}: the check did not run, so nothing was checked: ${result.detail}`,
    );
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
}

/** Reads the session's last assistant reply, checks it, and reports the result. Called once a session goes idle. */
async function reportOnSessionIdle(
  client: Client,
  config: PluginConfig,
  sessionID: string,
): Promise<void> {
  const text = await fetchLastAssistantText(client, sessionID);
  // A session that could not be read, or a turn with no assistant text at
  // all, gives nothing to check. The first case is already logged above;
  // the second is the one case where silence is right.
  if (text === undefined || text === "") return;

  const result = readResult(
    await runCheck(config, {
      sessionID,
      hook_event_name: "Stop",
      last_assistant_message: text,
    }),
  );
  await reportCheckResult(client, config, result);
}

export const server: Plugin = ({ client }, options) => {
  const config = readConfig(options);
  const hooks: Hooks = {
    event: async ({ event }) => {
      if (event.type !== "session.idle") return;
      await reportOnSessionIdle(client, config, event.properties.sessionID);
    },
  };
  return Promise.resolve(hooks);
};
