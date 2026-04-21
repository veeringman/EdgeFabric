ALTER TABLE virtual_iot_configs
ADD COLUMN IF NOT EXISTS event_definitions JSONB NOT NULL DEFAULT '[]'::jsonb,
ADD COLUMN IF NOT EXISTS sensor_state JSONB NOT NULL DEFAULT '{}'::jsonb,
ADD COLUMN IF NOT EXISTS sensor_log JSONB NOT NULL DEFAULT '[]'::jsonb;
