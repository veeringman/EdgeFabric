export interface LoginResponse {
  token: string;
  tenant_id: string;
  expires_in: number;
}

export interface BuildJob {
  id: string;
  build_type: string;
  status: string;
  source_ref: string;
  device_id?: string;
  runner_id?: string;
  queued_at?: string;
}

export interface BuildListResponse {
  builds: BuildJob[];
  total: number;
}

export interface BuildFilterState {
  status?: string;
  device?: string;
  source?: string;
}

export interface BuildLogsResponse {
  build_id: string;
  status: string;
  mode?: string;
  stdout?: string;
  stderr?: string;
}

export interface BuildArtifact {
  id: string;
  artifact_type: string;
  file_name: string;
  storage_path: string;
  size_bytes: number;
}

export interface BuildArtifactsResponse {
  build_id: string;
  artifacts: BuildArtifact[];
  total: number;
}
