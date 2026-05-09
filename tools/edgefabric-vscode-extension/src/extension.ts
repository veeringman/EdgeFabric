import * as vscode from "vscode";
import { execFile } from "child_process";
import { promisify } from "util";
import { EdgeFabricClient } from "./api";
import { BuildItem, BuildsTreeProvider } from "./buildsTree";

const execFileAsync = promisify(execFile);

export function activate(context: vscode.ExtensionContext): void {
  const client = new EdgeFabricClient(context);
  const output = vscode.window.createOutputChannel("EdgeFabric");
  const buildsProvider = new BuildsTreeProvider(client);

  context.subscriptions.push(
    output,
    vscode.window.createTreeView("edgefabricBuilds", { treeDataProvider: buildsProvider }),

    vscode.commands.registerCommand("edgefabric.login", async () => {
      try {
        const apiKey = await vscode.window.showInputBox({
          prompt: "Enter EdgeFabric API key",
          ignoreFocusOut: true,
          password: true,
        });
        if (!apiKey) {
          return;
        }

        const login = await client.loginWithApiKey(apiKey);
        vscode.window.showInformationMessage(`EdgeFabric login successful for tenant ${login.tenant_id}.`);
        buildsProvider.refresh();
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.logout", async () => {
      await client.logout();
      vscode.window.showInformationMessage("EdgeFabric session cleared.");
      buildsProvider.refresh();
    }),

    vscode.commands.registerCommand("edgefabric.refreshBuilds", () => {
      buildsProvider.refresh();
    }),

    vscode.commands.registerCommand("edgefabric.setBuildFilters", async () => {
      try {
        await ensureLoggedIn(client);
        const current = buildsProvider.getFilters();

        const statusPick = await vscode.window.showQuickPick(
          ["", "queued", "provisioning", "running", "succeeded", "failed", "cancelled"],
          {
            title: "Build status filter",
            placeHolder: current.status || "All statuses",
            ignoreFocusOut: true,
          }
        );

        if (statusPick === undefined) {
          return;
        }

        const device = await optionalInput("Device ID contains (optional)", current.device ?? "");
        const source = await optionalInput("Source ref contains (optional)", current.source ?? "");

        buildsProvider.setFilters({
          status: statusPick || undefined,
          device: device || undefined,
          source: source || undefined,
        });

        vscode.window.showInformationMessage("EdgeFabric build filters updated.");
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.clearBuildFilters", () => {
      buildsProvider.clearFilters();
      vscode.window.showInformationMessage("EdgeFabric build filters cleared.");
    }),

    vscode.commands.registerCommand("edgefabric.createIosStarterProject", async () => {
      try {
        const workspaceRoot = getWorkspaceRoot();
        const scriptUri = vscode.Uri.joinPath(workspaceRoot, "scripts", "create-ios-starter.sh");

        try {
          await vscode.workspace.fs.stat(scriptUri);
        } catch {
          throw new Error("Starter project generator script not found in this workspace.");
        }

        const appName = await requiredInput("App name", "EdgeStarter");
        const interfaceStyle = await requiredPick("Starter interface", ["swiftui", "uikit"]);
        const organizationId = await requiredInput("Organization identifier", "com.edgefabric.samples");
        const bundleId = await optionalInput("Bundle identifier override (optional)", "");
        const teamId = await optionalInput("Apple developer team ID (optional)", "");
        const deploymentTarget = await requiredInput("Deployment target", "17.0");
        const shouldArchive = await vscode.window.showQuickPick(
          [
            { label: "Create zip archive", value: true },
            { label: "Project folder only", value: false },
          ],
          {
            title: "Archive output",
            ignoreFocusOut: true,
          }
        );

        if (!shouldArchive) {
          return;
        }

        const outputRootUri = await pickStarterOutputRoot(workspaceRoot);
        if (!outputRootUri) {
          return;
        }

        const moduleName = sanitizeModuleName(appName);
        const projectDir = vscode.Uri.joinPath(outputRootUri, moduleName);
        const args = [
          scriptUri.fsPath,
          "--name",
          appName,
          "--interface",
          interfaceStyle,
          "--organization-id",
          organizationId,
          "--deployment-target",
          deploymentTarget,
          "--output-dir",
          outputRootUri.fsPath,
        ];

        if (bundleId) {
          args.push("--bundle-id", bundleId);
        }
        if (teamId) {
          args.push("--team-id", teamId);
        }
        if (shouldArchive.value) {
          args.push("--archive");
        }

        await vscode.window.withProgress(
          {
            location: vscode.ProgressLocation.Notification,
            title: `Creating iOS starter project ${appName}`,
          },
          async () => {
            const result = await execFileAsync("bash", args, { cwd: workspaceRoot.fsPath });
            if (result.stdout.trim()) {
              output.appendLine(result.stdout.trim());
            }
            if (result.stderr.trim()) {
              output.appendLine(result.stderr.trim());
            }
          }
        );

        const action = await vscode.window.showInformationMessage(
          `Starter project created at ${projectDir.fsPath}`,
          "Open Project Folder",
          "Reveal In Finder"
        );

        if (action === "Open Project Folder") {
          await vscode.commands.executeCommand("vscode.openFolder", projectDir, { forceNewWindow: true });
        } else if (action === "Reveal In Finder") {
          await vscode.commands.executeCommand("revealFileInOS", projectDir);
        }
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.submitIotDeploy", async () => {
      try {
        await ensureLoggedIn(client);

        const sourceRef = await requiredInput("Source ref", "project-main");
        const projectPath = await requiredInput("Project path on runner", "/workspace/project");
        const targetDeviceId = await requiredInput("Target IoT device ID", "");
        const boardProfile = await optionalInput("Board profile", "esp32c6");
        const deployCommand = await optionalInput("Deploy command", "idf.py build && idf.py flash");

        const payload = {
          source_ref: sourceRef,
          project_path: projectPath,
          target_device_id: targetDeviceId,
          board_profile: boardProfile || undefined,
          deploy_command: deployCommand || undefined,
          region: client.defaultRegion || undefined,
          required_runner_capabilities: [
            "cicd_runner",
            "linux_runner",
            "iot_toolchain",
            "iot_deploy",
            "artifact_upload",
          ],
          required_runner_labels: ["iot"],
          lease_ttl_seconds: 1800,
          ssh_user: client.defaultSshUser,
        };

        const result = await client.submitIotDeploy(payload);
        output.appendLine(`IoT deploy submitted: ${JSON.stringify(result)}`);
        vscode.window.showInformationMessage("EdgeFabric IoT build/deploy submitted.");
        buildsProvider.refresh();
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.submitIosBuild", async () => {
      try {
        await ensureLoggedIn(client);

        const sourceRef = await requiredInput("Source ref", "project-main");
        const projectPath = await requiredInput("Project path on runner", "/Users/builder/work/ios-app");
        const scheme = await requiredInput("Xcode scheme", "App");
        const configuration = await requiredPick("Build configuration", ["Debug", "Release"]);
        const targetDeviceId = await requiredInput("Target iOS device ID", "");

        const payload = {
          source_ref: sourceRef,
          project_path: projectPath,
          scheme,
          configuration,
          target_device_id: targetDeviceId,
          region: client.defaultRegion || undefined,
          required_runner_capabilities: [
            "cicd_runner",
            "xcode",
            "artifact_upload",
          ],
          required_runner_labels: ["ios"],
          lease_ttl_seconds: 1800,
          ssh_user: client.defaultSshUser,
        };

        const result = await client.submitIosBuild(payload);
        output.appendLine(`iOS build submitted: ${JSON.stringify(result)}`);
        vscode.window.showInformationMessage("EdgeFabric iOS build/deploy submitted.");
        buildsProvider.refresh();
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.submitQuantumRun", async () => {
      try {
        await ensureLoggedIn(client);

        const sourceRef = await requiredInput("Source ref", "project-main");
        const projectPath = await requiredInput("Project path on runner", "/workspace/project");
        const backendKind = await requiredPick("Backend kind", ["virtual", "real"]);
        const backendName = await optionalInput("Backend name", "");
        const veerosHostHint = await optionalInput("VeerOS host hint", "");
        const runCommand = await optionalInput("Run command", "cargo run --release");

        const requiredCapabilities = ["cicd_runner", "linux_runner", "quantum_sdk", "artifact_upload"];
        if (backendKind === "virtual") {
          requiredCapabilities.push("quantum_simulator");
        } else {
          requiredCapabilities.push("veeros_host");
        }

        const payload = {
          source_ref: sourceRef,
          project_path: projectPath,
          run_command: runCommand || undefined,
          backend_kind: backendKind,
          backend_name: backendName || undefined,
          veeros_host_hint: veerosHostHint || undefined,
          region: client.defaultRegion || undefined,
          required_runner_capabilities: requiredCapabilities,
          required_runner_labels: ["quantum"],
          lease_ttl_seconds: 1800,
          ssh_user: client.defaultSshUser,
        };

        const result = await client.submitQuantumRun(payload);
        output.appendLine(`Quantum run submitted: ${JSON.stringify(result)}`);
        vscode.window.showInformationMessage("EdgeFabric quantum build/run submitted.");
        buildsProvider.refresh();
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.viewBuildLogs", async (buildIdArg?: string) => {
      try {
        await ensureLoggedIn(client);

        let buildId = buildIdArg;
        if (!buildId) {
          const builds = await client.listBuilds(100, 0);
          const pick = await vscode.window.showQuickPick(
            builds.builds.map((b) => ({
              label: `${b.status} - ${b.id.slice(0, 8)}`,
              description: `${b.build_type} | ${b.source_ref}`,
              value: b.id,
            })),
            { title: "Select build" }
          );
          buildId = pick?.value;
        }

        if (!buildId) {
          return;
        }

        const logs = await client.getBuildLogs(buildId);
        output.clear();
        output.appendLine(`Build ID: ${logs.build_id}`);
        output.appendLine(`Status: ${logs.status}`);
        output.appendLine(`Mode: ${logs.mode ?? "unknown"}`);
        output.appendLine("\n===== STDOUT =====");
        output.appendLine(logs.stdout ?? "");
        output.appendLine("\n===== STDERR =====");
        output.appendLine(logs.stderr ?? "");
        output.show(true);
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.downloadBuildArtifact", async (item?: BuildItem) => {
      try {
        await ensureLoggedIn(client);

        let buildId = item?.build.id;
        if (!buildId) {
          const builds = await client.listBuilds(100, 0, buildsProvider.getFilters());
          const pick = await vscode.window.showQuickPick(
            builds.builds.map((b) => ({
              label: `${b.status} - ${b.id.slice(0, 8)}`,
              description: `${b.build_type} | ${b.source_ref}`,
              value: b.id,
            })),
            { title: "Select build for artifact download" }
          );
          buildId = pick?.value;
        }

        if (!buildId) {
          return;
        }

        const artifactList = await client.getBuildArtifacts(buildId);
        if (!artifactList.artifacts.length) {
          vscode.window.showWarningMessage("No artifacts found for the selected build.");
          return;
        }

        const types = Array.from(
          new Set(artifactList.artifacts.map((a) => a.artifact_type).filter((v) => v.trim().length > 0))
        ).sort((a, b) => a.localeCompare(b));

        let selectedType: string | undefined;
        if (types.length > 1) {
          const typePick = await vscode.window.showQuickPick(
            [
              { label: "All types", value: "" },
              ...types.map((t) => ({ label: t, value: t })),
            ],
            {
              title: "Artifact type filter",
              placeHolder: "Choose type before selecting artifact",
            }
          );

          if (!typePick) {
            return;
          }

          selectedType = typePick.value || undefined;
        }

        const typeRank: Record<string, number> = {
          ipa: 0,
          apk: 1,
          binary: 2,
          xcresult: 3,
          log: 4,
        };

        const filteredArtifacts = artifactList.artifacts.filter((a) => {
          if (!selectedType) {
            return true;
          }
          return a.artifact_type === selectedType;
        });

        if (!filteredArtifacts.length) {
          vscode.window.showWarningMessage("No artifacts match the selected type filter.");
          return;
        }

        const sortedArtifacts = [...filteredArtifacts].sort((a, b) => {
          const bySize = b.size_bytes - a.size_bytes;
          if (bySize !== 0) {
            return bySize;
          }

          const aRank = typeRank[a.artifact_type.toLowerCase()] ?? 99;
          const bRank = typeRank[b.artifact_type.toLowerCase()] ?? 99;
          if (aRank !== bRank) {
            return aRank - bRank;
          }

          return a.file_name.localeCompare(b.file_name);
        });

        const selected = await vscode.window.showQuickPick(
          sortedArtifacts.map((a) => ({
            label: `${artifactIcon(a.artifact_type)} ${a.file_name}`,
            description: `${a.artifact_type} | ${formatBytes(a.size_bytes)}`,
            detail: a.storage_path,
            value: a,
          })),
          {
            title: "Select artifact",
            placeHolder: "Sorted by size (largest first), then artifact type",
          }
        );

        if (!selected) {
          return;
        }

        const data = await client.downloadArtifact(selected.value.storage_path);

        const saveTarget = await vscode.window.showSaveDialog({
          saveLabel: "Save Artifact",
          defaultUri: vscode.Uri.file(selected.value.file_name),
        });

        if (!saveTarget) {
          return;
        }

        await vscode.workspace.fs.writeFile(saveTarget, data);
        vscode.window.showInformationMessage(`Artifact saved to ${saveTarget.fsPath}`);
      } catch (error) {
        showError(error);
      }
    }),

    vscode.commands.registerCommand("edgefabric.downloadAllBuildArtifacts", async (item?: BuildItem) => {
      try {
        await ensureLoggedIn(client);

        let buildId = item?.build.id;
        if (!buildId) {
          const builds = await client.listBuilds(100, 0, buildsProvider.getFilters());
          const pick = await vscode.window.showQuickPick(
            builds.builds.map((b) => ({
              label: `${b.status} - ${b.id.slice(0, 8)}`,
              description: `${b.build_type} | ${b.source_ref}`,
              value: b.id,
            })),
            { title: "Select build for bulk artifact download" }
          );
          buildId = pick?.value;
        }

        if (!buildId) {
          return;
        }

        const artifactList = await client.getBuildArtifacts(buildId);
        if (!artifactList.artifacts.length) {
          vscode.window.showWarningMessage("No artifacts found for the selected build.");
          return;
        }

        const folderPick = await vscode.window.showOpenDialog({
          canSelectFiles: false,
          canSelectFolders: true,
          canSelectMany: false,
          openLabel: "Select download folder",
        });

        if (!folderPick || !folderPick[0]) {
          return;
        }

        const baseDir = folderPick[0];
        const buildDir = vscode.Uri.joinPath(baseDir, `edgefabric-${buildId.slice(0, 8)}`);
        await vscode.workspace.fs.createDirectory(buildDir);

        let downloadedCount = 0;
        for (const artifact of artifactList.artifacts) {
          try {
            const data = await client.downloadArtifact(artifact.storage_path);
            const safeName = sanitizeFileName(artifact.file_name || `${artifact.id}.bin`);
            const target = vscode.Uri.joinPath(buildDir, safeName);
            await vscode.workspace.fs.writeFile(target, data);
            downloadedCount += 1;
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            output.appendLine(`Artifact download failed for ${artifact.file_name}: ${msg}`);
          }
        }

        vscode.window.showInformationMessage(
          `Downloaded ${downloadedCount}/${artifactList.artifacts.length} artifacts to ${buildDir.fsPath}`
        );
      } catch (error) {
        showError(error);
      }
    })
  );
}

export function deactivate(): void {
  // no-op
}

async function ensureLoggedIn(client: EdgeFabricClient): Promise<void> {
  const token = await client.getToken();
  if (!token) {
    throw new Error("Not logged in. Run 'EdgeFabric: Login'.");
  }
}

async function requiredInput(prompt: string, value: string): Promise<string> {
  const result = await vscode.window.showInputBox({
    prompt,
    value,
    ignoreFocusOut: true,
    validateInput: (v: string) => (v.trim().length === 0 ? "Required" : undefined),
  });

  if (!result || result.trim().length === 0) {
    throw new Error(`${prompt} is required.`);
  }

  return result.trim();
}

async function optionalInput(prompt: string, value: string): Promise<string> {
  const result = await vscode.window.showInputBox({
    prompt,
    value,
    ignoreFocusOut: true,
  });

  return (result ?? "").trim();
}

async function requiredPick(prompt: string, items: string[]): Promise<string> {
  const picked = await vscode.window.showQuickPick(items, {
    title: prompt,
    ignoreFocusOut: true,
  });

  if (!picked) {
    throw new Error(`${prompt} is required.`);
  }

  return picked;
}

function showError(error: unknown): void {
  const msg = error instanceof Error ? error.message : String(error);
  void vscode.window.showErrorMessage(`EdgeFabric: ${msg}`);
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "unknown";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }

  return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[idx]}`;
}

function artifactIcon(artifactType: string): string {
  const t = artifactType.toLowerCase();
  if (t.includes("ipa") || t.includes("apk") || t.includes("binary")) {
    return "$(package)";
  }
  if (t.includes("log")) {
    return "$(output)";
  }
  if (t.includes("result") || t.includes("report")) {
    return "$(graph)";
  }
  return "$(file)";
}

function sanitizeFileName(fileName: string): string {
  const trimmed = fileName.trim();
  if (!trimmed) {
    return "artifact.bin";
  }
  return trimmed.replace(/[\\/:*?"<>|]/g, "_");
}

function sanitizeModuleName(value: string): string {
  const cleaned = value.replace(/[^a-zA-Z0-9_]/g, "");
  if (!cleaned) {
    throw new Error("App name must contain at least one alphanumeric character.");
  }
  return cleaned;
}

function getWorkspaceRoot(): vscode.Uri {
  const root = vscode.workspace.workspaceFolders?.[0]?.uri;
  if (!root) {
    throw new Error("Open the EdgeFabric workspace before creating a starter project.");
  }
  return root;
}

async function pickStarterOutputRoot(workspaceRoot: vscode.Uri): Promise<vscode.Uri | undefined> {
  const choice = await vscode.window.showQuickPick(
    [
      {
        label: "Use workspace generated-ios folder",
        description: vscode.Uri.joinPath(workspaceRoot, "generated-ios").fsPath,
        value: vscode.Uri.joinPath(workspaceRoot, "generated-ios"),
      },
      {
        label: "Choose a different folder",
        description: "Pick any local destination",
        value: undefined,
      },
    ],
    {
      title: "Starter project destination",
      ignoreFocusOut: true,
    }
  );

  if (!choice) {
    return undefined;
  }

  if (choice.value) {
    return choice.value;
  }

  const folder = await vscode.window.showOpenDialog({
    canSelectFiles: false,
    canSelectFolders: true,
    canSelectMany: false,
    openLabel: "Select output folder",
    defaultUri: workspaceRoot,
  });

  return folder?.[0];
}
