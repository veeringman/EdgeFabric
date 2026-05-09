import * as vscode from "vscode";
import {
  BuildArtifactsResponse,
  BuildFilterState,
  BuildListResponse,
  BuildLogsResponse,
  LoginResponse,
} from "./types";

const TOKEN_KEY = "edgefabric.token";
const TENANT_KEY = "edgefabric.tenantId";

export class EdgeFabricClient {
  constructor(private readonly context: vscode.ExtensionContext) {}

  get baseUrl(): string {
    const cfg = vscode.workspace.getConfiguration("edgefabric");
    return cfg.get<string>("apiBaseUrl", "http://localhost:6060/api/v1").replace(/\/$/, "");
  }

  get defaultRegion(): string {
    return vscode.workspace.getConfiguration("edgefabric").get<string>("defaultRegion", "");
  }

  get defaultSshUser(): string {
    const configured = vscode.workspace.getConfiguration("edgefabric").get<string>("defaultSshUser", "").trim();
    if (configured) {
      return configured;
    }

    return process.env.USER ?? process.env.USERNAME ?? "builder";
  }

  async getToken(): Promise<string | undefined> {
    return this.context.secrets.get(TOKEN_KEY);
  }

  async loginWithApiKey(apiKey: string): Promise<LoginResponse> {
    const response = await fetch(`${this.baseUrl}/auth/login`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ api_key: apiKey }),
    });

    if (!response.ok) {
      const text = await response.text();
      throw new Error(`Login failed (${response.status}): ${text}`);
    }

    const data = (await response.json()) as LoginResponse;
    await this.context.secrets.store(TOKEN_KEY, data.token);
    await this.context.globalState.update(TENANT_KEY, data.tenant_id);
    return data;
  }

  async logout(): Promise<void> {
    await this.context.secrets.delete(TOKEN_KEY);
    await this.context.globalState.update(TENANT_KEY, undefined);
  }

  async listBuilds(limit = 100, offset = 0, filters?: BuildFilterState): Promise<BuildListResponse> {
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

    return this.request<BuildListResponse>(`/builds?${params.toString()}`);
  }

  async getBuildLogs(buildId: string): Promise<BuildLogsResponse> {
    return this.request<BuildLogsResponse>(`/builds/${encodeURIComponent(buildId)}/logs`);
  }

  async getBuildArtifacts(buildId: string): Promise<BuildArtifactsResponse> {
    return this.request<BuildArtifactsResponse>(`/builds/${encodeURIComponent(buildId)}/artifacts`);
  }

  async downloadArtifact(storagePath: string): Promise<Uint8Array> {
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

  async submitIotDeploy(payload: Record<string, unknown>): Promise<unknown> {
    return this.request("/builds/iot/deploy", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  async submitIosBuild(payload: Record<string, unknown>): Promise<unknown> {
    return this.request("/builds/ios", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  async submitQuantumRun(payload: Record<string, unknown>): Promise<unknown> {
    return this.request("/builds/quantum/run", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  }

  private async request<T = unknown>(path: string, init?: RequestInit): Promise<T> {
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

    return (await response.json()) as T;
  }
}
