"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.BuildsTreeProvider = exports.BuildItem = void 0;
const vscode = __importStar(require("vscode"));
class BuildItem extends vscode.TreeItem {
    build;
    constructor(build) {
        super(`${build.status} - ${build.id.slice(0, 8)}`, vscode.TreeItemCollapsibleState.None);
        this.build = build;
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
exports.BuildItem = BuildItem;
class BuildsTreeProvider {
    client;
    onDidChangeTreeDataEmitter = new vscode.EventEmitter();
    onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;
    filters = {};
    constructor(client) {
        this.client = client;
    }
    setFilters(filters) {
        this.filters = {
            status: filters.status?.trim() || undefined,
            device: filters.device?.trim() || undefined,
            source: filters.source?.trim() || undefined,
        };
        this.refresh();
    }
    clearFilters() {
        this.filters = {};
        this.refresh();
    }
    getFilters() {
        return { ...this.filters };
    }
    refresh() {
        this.onDidChangeTreeDataEmitter.fire();
    }
    getTreeItem(element) {
        return element;
    }
    async getChildren() {
        const builds = await this.client.listBuilds(100, 0, this.filters);
        return builds.builds.map((b) => new BuildItem(b));
    }
}
exports.BuildsTreeProvider = BuildsTreeProvider;
//# sourceMappingURL=buildsTree.js.map