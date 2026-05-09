import * as vscode from "vscode";
import { EdgeFabricClient } from "./api";
import { BuildFilterState, BuildJob } from "./types";

export class BuildItem extends vscode.TreeItem {
  constructor(public readonly build: BuildJob) {
    super(`${build.status} - ${build.id.slice(0, 8)}`, vscode.TreeItemCollapsibleState.None);
    this.description = `${build.build_type} ${build.device_id ? `| ${build.device_id}` : ""}`;
    this.tooltip = `${build.id}\n${build.source_ref}`;
    this.contextValue = "edgefabricBuild";
    this.command = {
      command: "edgefabric.viewBuildLogs",
      title: "View Build Logs",
      arguments: [build.id],
    };

    switch (build.status) {
      case "succeeded":
        this.iconPath = new vscode.ThemeIcon("pass");
        break;
      case "failed":
      case "cancelled":
        this.iconPath = new vscode.ThemeIcon("error");
        break;
      case "running":
        this.iconPath = new vscode.ThemeIcon("sync~spin");
        break;
      default:
        this.iconPath = new vscode.ThemeIcon("clock");
        break;
    }
  }
}

export class BuildsTreeProvider implements vscode.TreeDataProvider<BuildItem> {
  private readonly onDidChangeTreeDataEmitter = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;
  private filters: BuildFilterState = {};

  constructor(private readonly client: EdgeFabricClient) {}

  setFilters(filters: BuildFilterState): void {
    this.filters = {
      status: filters.status?.trim() || undefined,
      device: filters.device?.trim() || undefined,
      source: filters.source?.trim() || undefined,
    };
    this.refresh();
  }

  clearFilters(): void {
    this.filters = {};
    this.refresh();
  }

  getFilters(): BuildFilterState {
    return { ...this.filters };
  }

  refresh(): void {
    this.onDidChangeTreeDataEmitter.fire();
  }

  getTreeItem(element: BuildItem): vscode.TreeItem {
    return element;
  }

  async getChildren(): Promise<BuildItem[]> {
    const builds = await this.client.listBuilds(100, 0, this.filters);
    return builds.builds.map((b) => new BuildItem(b));
  }
}
