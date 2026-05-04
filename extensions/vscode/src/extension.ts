import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";
import { spawnSync } from "child_process";
import * as path from "path";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("phantomdep.restartServer", async () => {
      await restart(context);
    }),
    vscode.commands.registerCommand("phantomdep.runDoctor", () => {
      const term = vscode.window.createTerminal("PhantomDep doctor");
      const bin = resolveBinary();
      term.show(true);
      term.sendText(`${bin} doctor`);
    })
  );

  await start(context);
}

export async function deactivate(): Promise<void> {
  if (!client) {
    return;
  }
  await client.stop();
}

async function start(context: vscode.ExtensionContext): Promise<void> {
  const config = vscode.workspace.getConfiguration("phantomdep");
  const bin = resolveBinary();

  if (!binaryWorks(bin)) {
    void vscode.window.showWarningMessage(
      `PhantomDep: cannot run \`${bin}\`. Install from https://github.com/openintelligence-labs/phantomdep#install or set phantomdep.binaryPath in settings.`
    );
    return;
  }

  const args = ["lsp"];
  const phantomDbPath = config.get<string>("phantomDbPath", "").trim();
  const env = { ...process.env };
  if (phantomDbPath) {
    env["PHANTOMDEP_DB"] = phantomDbPath;
  }

  const serverOptions: ServerOptions = {
    run: { command: bin, args, transport: TransportKind.stdio, options: { env } },
    debug: { command: bin, args, transport: TransportKind.stdio, options: { env } },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "python" },
      { scheme: "file", language: "javascript" },
      { scheme: "file", language: "javascriptreact" },
      { scheme: "file", language: "typescript" },
      { scheme: "file", language: "typescriptreact" },
    ],
    synchronize: {
      configurationSection: "phantomdep",
    },
    outputChannelName: "PhantomDep",
  };

  client = new LanguageClient(
    "phantomdep",
    "PhantomDep",
    serverOptions,
    clientOptions
  );

  context.subscriptions.push({ dispose: () => void client?.stop() });

  try {
    await client.start();
  } catch (err) {
    void vscode.window.showErrorMessage(
      `PhantomDep: failed to start language server: ${err}`
    );
  }
}

async function restart(context: vscode.ExtensionContext): Promise<void> {
  if (client) {
    try {
      await client.stop();
    } catch {
      /* ignore */
    }
    client = undefined;
  }
  await start(context);
}

function resolveBinary(): string {
  const config = vscode.workspace.getConfiguration("phantomdep");
  const explicit = config.get<string>("binaryPath", "").trim();
  if (explicit) {
    if (path.isAbsolute(explicit)) {
      return explicit;
    }
    return explicit;
  }
  return "phantomdep";
}

function binaryWorks(bin: string): boolean {
  try {
    const r = spawnSync(bin, ["--version"], { encoding: "utf8" });
    return r.status === 0;
  } catch {
    return false;
  }
}
