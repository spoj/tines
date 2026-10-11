// Puts this Pi session in letmeknow: an interactive one as the session `pi-<end of its id>`, removed when it ends in no
// group, and any session started with `--letmeknow <handle>` as that one, kept. What the session prints reaches the agent
// as a message that starts a turn; its `ready` only names the session, which the system prompt and status line show.
import { type ChildProcess, execFile, spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { promisify } from "node:util";
import type { TextContent } from "@earendil-works/pi-ai";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const LETMEKNOW = join(homedir(), ".tines", "bin", process.platform === "win32" ? "letmeknow.exe" : "letmeknow");
const run = promisify(execFile);

export default function tines(pi: ExtensionAPI): void {
	let session: { handle: string; listen: ChildProcess; removable: boolean } | undefined;
	let pending: TextContent[] | undefined;

	pi.registerFlag("letmeknow", { type: "string", description: "Put this session in letmeknow, in any mode, as this session handle" });

	function notify(text: string): void {
		const queued = pending !== undefined;
		pending ??= [];
		// Pi retains this array until message_start; later lines join the same queued message.
		pending.push({ type: "text", text });
		if (!queued) pi.sendMessage({ customType: "letmeknow", content: pending, display: true }, { deliverAs: "steer", triggerTurn: true });
	}

	pi.on("message_start", ({ message }) => {
		if (message.role === "custom" && message.customType === "letmeknow" && message.content === pending) pending = undefined;
	});
	pi.on("agent_settled", (_event, ctx) => {
		if (ctx.isIdle()) pending = undefined;
	});

	pi.on("session_start", (_event, ctx) => {
		const flag = pi.getFlag("letmeknow");
		if (typeof flag !== "string" && ctx.mode !== "tui") return;
		const handle = typeof flag === "string" ? flag : `pi-${ctx.sessionManager.getSessionId().slice(-12)}`;
		const listen = spawn(LETMEKNOW, ["--session", handle, "listen"], { stdio: ["ignore", "pipe", "pipe"] });
		const current = { handle, listen, removable: typeof flag !== "string" };
		session = current;
		process.env.LETMEKNOW_SESSION = handle;
		ctx.ui.setStatus("letmeknow", `letmeknow: ${handle}`);
		let errors = "";
		listen.stderr!.on("data", chunk => (errors = (errors + chunk).slice(-2000)));
		listen.on("error", error => ctx.ui.notify(`letmeknow: ${error.message}; run tines update`, "error"));
		listen.on("exit", code => session === current && ctx.ui.notify(`letmeknow stopped (exit ${code}): ${errors.trim()}`, "error"));
		createInterface({ input: listen.stdout! }).on("line", line => session === current && JSON.parse(line).type !== "ready" && notify(line));
	});

	pi.on("before_agent_start", event => {
		if (!session) return;
		event.systemPromptOptions.sections.letmeknow = `You are in letmeknow as its session ${session.handle}, which runs already: what it prints reaches you as messages. Read \`npx -y @letmeknow/cli@0.14 skill\` before you first use letmeknow.`;
	});

	pi.on("session_shutdown", async (_event, ctx) => {
		if (!session) return;
		const { handle, listen, removable } = session;
		session = undefined;
		delete process.env.LETMEKNOW_SESSION;
		ctx.ui.setStatus("letmeknow", undefined);
		const { sessions } = JSON.parse((await run(LETMEKNOW, ["sessions"])).stdout) as { sessions: { handle: string; groups: unknown[] }[] };
		if (removable && sessions.some(s => s.handle === handle && !s.groups.length)) await run(LETMEKNOW, ["sessions", "remove", handle]);
		else listen.kill();
	});
}
