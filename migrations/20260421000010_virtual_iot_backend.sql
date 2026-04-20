ALTER TABLE virtual_iot_configs
ADD COLUMN IF NOT EXISTS backend JSONB NOT NULL DEFAULT '{}'::jsonb;