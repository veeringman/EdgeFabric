-- Test cases within a job — tracks individual test execution and results.
CREATE TABLE test_cases (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id      UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    device_id   UUID REFERENCES devices(id),
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'running', 'passed', 'failed', 'skipped', 'retrying')),
    attempt     INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 2,
    started_at  TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    duration_ms INTEGER,
    error       TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_test_cases_job    ON test_cases (job_id);
CREATE INDEX idx_test_cases_status ON test_cases (status);
CREATE INDEX idx_test_cases_device ON test_cases (device_id);

-- Artifact storage metadata (logs, screenshots, JUnit XML, etc.)
CREATE TABLE artifacts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id       UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    test_case_id UUID REFERENCES test_cases(id) ON DELETE SET NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('log', 'screenshot', 'video', 'junit_xml', 'other')),
    path         TEXT NOT NULL,
    size_bytes   BIGINT NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_artifacts_job  ON artifacts (job_id);
CREATE INDEX idx_artifacts_test ON artifacts (test_case_id);

-- Add webhook_url column to jobs for completion callbacks
ALTER TABLE jobs ADD COLUMN webhook_url TEXT;
-- Add test_suite_path to know what to run
ALTER TABLE jobs ADD COLUMN test_suite TEXT NOT NULL DEFAULT '';
