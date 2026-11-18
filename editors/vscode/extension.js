// Starts glassmere-designer on the .glass file being edited. The designer
// follows the file on disk, so the preview changes on every save; with
// auto save set to `afterDelay` that is as the view is typed.
const vscode = require("vscode");
const { spawn } = require("child_process");
const path = require("path");

/** One designer per file, by path. */
const running = new Map();

function showPreview() {
  const document = vscode.window.activeTextEditor?.document;
  if (!document || document.languageId !== "glass" || document.isUntitled) {
    vscode.window.showInformationMessage("Open a saved .glass file to preview it.");
    return;
  }
  const file = document.uri.fsPath;
  if (running.has(file)) {
    vscode.window.showInformationMessage("The preview of this file is already open.");
    return;
  }
  const settings = vscode.workspace.getConfiguration("glassmere", document.uri);
  const program = settings.get("designerPath") || "glassmere-designer";
  const args = [file];
  const sheet = settings.get("stylesheet");
  if (sheet) {
    const folder = vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath;
    args.push("--style", folder ? path.resolve(folder, sheet) : sheet);
  }
  const child = spawn(program, args, { cwd: path.dirname(file), stdio: "ignore" });
  running.set(file, child);
  child.on("error", (error) => {
    running.delete(file);
    vscode.window.showErrorMessage(
      `Could not start ${program}: ${error.message}. Install it with "cargo install glassmere-designer", or set glassmere.designerPath.`
    );
  });
  child.on("exit", () => running.delete(file));
}

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("glassmere.showPreview", showPreview),
    { dispose: () => running.forEach((child) => child.kill()) }
  );
}

module.exports = { activate, deactivate() {} };
