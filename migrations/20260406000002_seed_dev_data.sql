-- EdgeFabric — Development Seed Data

-- Default tenant for local development
INSERT INTO tenants (id, name, api_key, max_sessions)
VALUES (
    'a0000000-0000-0000-0000-000000000001',
    'EdgeFabric Dev',
    'efdev-key-00000000',
    20
) ON CONFLICT (name) DO NOTHING;

-- Sample Android devices
INSERT INTO devices (id, name, os, os_version, kind, status, region, capabilities) VALUES
    ('d0000000-0000-0000-0000-000000000001', 'Pixel 8 Pro',     'android', '14',  'real',    'available', 'us-west', ARRAY['camera','nfc','5g']),
    ('d0000000-0000-0000-0000-000000000002', 'Samsung Galaxy S24', 'android', '14', 'real',   'available', 'us-west', ARRAY['camera','nfc','5g']),
    ('d0000000-0000-0000-0000-000000000003', 'Android Emu API 34', 'android', '14', 'virtual','available', 'us-west', ARRAY['camera'])
ON CONFLICT DO NOTHING;

-- Sample iOS devices
INSERT INTO devices (id, name, os, os_version, kind, status, region, capabilities) VALUES
    ('d0000000-0000-0000-0000-000000000004', 'iPhone 15 Pro',    'ios', '17.4', 'real',    'available', 'us-east', ARRAY['camera','face_id','5g']),
    ('d0000000-0000-0000-0000-000000000005', 'iPhone SE (3rd)',  'ios', '17.4', 'real',    'offline',   'us-east', ARRAY['camera','touch_id']),
    ('d0000000-0000-0000-0000-000000000006', 'iOS Sim 17',       'ios', '17.4', 'virtual', 'available', 'us-east', ARRAY['camera'])
ON CONFLICT DO NOTHING;

-- Sample IoT devices
INSERT INTO devices (id, name, os, os_version, kind, status, region, capabilities) VALUES
    ('d0000000-0000-0000-0000-000000000007', 'RPi 4 Gateway',   'iot', 'bullseye', 'real', 'available', 'eu-west', ARRAY['gpio','bluetooth','wifi']),
    ('d0000000-0000-0000-0000-000000000008', 'ESP32 DevKit',     'iot', 'esp-idf5', 'real', 'offline',  'eu-west', ARRAY['wifi','bluetooth'])
ON CONFLICT DO NOTHING;
