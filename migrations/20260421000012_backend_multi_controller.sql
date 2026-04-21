-- Add controller_types array to virtual_iot_backends to support multi-controller selection
ALTER TABLE virtual_iot_backends
ADD COLUMN IF NOT EXISTS controller_types TEXT[] NOT NULL DEFAULT '{}';

-- Populate from existing controller_type for all existing rows
UPDATE virtual_iot_backends
SET controller_types = ARRAY[controller_type]
WHERE controller_types = '{}';

CREATE INDEX IF NOT EXISTS idx_virtual_iot_backends_controller_types
    ON virtual_iot_backends USING GIN (controller_types);
