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
exports.EdgeFabricClient = void 0;
const vscode = __importStar(require("vscode"));
const TOKEN_KEY = "edgefabric.token";
const TENANT_KEY = "edgefabric.tenantId";
class EdgeFabricClient {
    context;
    constructor(context) {
        this.context = context;
    }
    get baseUrl() {
        const cfg = vscode.workspace.getConfiguration("edgefabric");
        return cfg.get("apiBaseUrl", "http://localhost:6060/api/v1").replace(/\/$/, "");
    }
    get defaultRegion() {
        return vscode.workspace.getConfiguration("edgefabric").get("defaultRegion", "");
    }
    get defaultSshUser() {
        const configured = vscode.workspace.getConfiguration("edgefabric").get("defaultSshUser", "").trim();
        if (configured) {
            return configured;
        }
        return process.env.USER ?? process.env.USERNAME ?? "builder";
    }
    async getToken() {
        return this.context.secrets.get(TOKEN_KEY);
    }
    async loginWithApiKey(apiKey) {
        const response = await fetch(`${this.baseUrl}/auth/login`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ api_key: apiKey }),
        });
        if (!response.ok) {
            const text = await response.text();
            throw new Error(`Login failed (${response.status}): ${text}`);
        }
        const data = (await response.json());
        await this.context.secrets.store(TOKEN_KEY, data.token);
        await this.context.globalState.update(TENANT_KEY, data.tenant_id);
        return data;
    }
    async logout() {
        await this.context.secrets.delete(TOKEN_KEY);
        await this.context.globalState.update(TENANT_KEY, undefined);
    }
    async listBuilds(limit = 100, offset = 0, filters) {
        const params = new URLSearchParams({
            limit: String(limit),
            offset: String(offset),
        });
        if (filters?.status) {
            params.set("status", filters.status);
        }
        if (filters?.device) {
            params.set("device", filters.device);
        }
        if (filters?.source) {
            params.set("source", filters.source);
        }
        return this.request(`/builds?${params.toString()}`);
    }
    async getBuildLogs(buildId) {
        return this.request(`/builds/${encodeURIComponent(buildId)}/logs`);
    }
    async getBuildArtifacts(buildId) {
        return this.request(`/builds/${encodeURIComponent(buildId)}/artifacts`);
    }
    async downloadArtifact(storagePath) {
        const token = await this.getToken();
        if (!token) {
            throw new Error("Not logged in. Run 'EdgeFabric: Login' first.");
        }
        let url = storagePath.trim();
        if (!/^https?:\/\//i.test(url)) {
            url = `${this.baseUrl}/${url.replace(/^\/+/, "")}`;
        }
        const response = await fetch(url, {
            method: "GET",
            headers: {
                Authorization: `Bearer ${token}`,
            },
        });
        if (!response.ok) {
            const text = await response.text();
            throw new Error(`Artifact download failed (${response.status}): ${text}`);
        }
        const buffer = await response.arrayBuffer();
        return new Uint8Array(buffer);
    }
    async submitIotDeploy(payload) {
        return this.request("/builds/iot/deploy", {
            method: "POST",
            body: JSON.stringify(payload),
        });
    }
    async submitIosBuild(payload) {
        return this.request("/builds/ios", {
            method: "POST",
            body: JSON.stringify(payload),
        });
    }
    async submitQuantumRun(payload) {
        return this.request("/builds/quantum/run", {
            method: "POST",
            body: JSON.stringify(payload),
        });
    }
    async request(path, init) {
        const token = await this.getToken();
        if (!token) {
            throw new Error("Not logged in. Run 'EdgeFabric: Login' first.");
        }
        const response = await fetch(`${this.baseUrl}${path}`, {
            ...init,
            headers: {
                "Content-Type": "application/json",
                Authorization: `Bearer ${token}`,
                ...(init?.headers ?? {}),
            },
        });
        if (!response.ok) {
            const text = await response.text();
            if (response.status === 401) {
                throw new Error("Session expired. Please run 'EdgeFabric: Login' again.");
            }
            throw new Error(`API error (${response.status}): ${text}`);
        }
        return (await response.json());
    }
}
exports.EdgeFabricClient = EdgeFabricClient;
//# sourceMappingURL=api.js.map