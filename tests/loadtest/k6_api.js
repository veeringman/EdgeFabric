// EdgeFabric API Load Test — k6 script
// Run with: k6 run tests/loadtest/k6_api.js
//
// Prerequisites:
//   - Running EdgeFabric API server at BASE_URL
//   - Valid API key in API_KEY env var
//
// Usage:
//   k6 run --env BASE_URL=http://localhost:8080 --env API_KEY=your-key tests/loadtest/k6_api.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const BASE_URL = __ENV.BASE_URL || 'http://localhost:8080';
const API_KEY = __ENV.API_KEY || '';

// Custom metrics
const errorRate = new Rate('errors');
const healthLatency = new Trend('health_latency');
const authLatency = new Trend('auth_latency');
const devicesLatency = new Trend('devices_latency');

export const options = {
  stages: [
    { duration: '10s', target: 10 },   // Ramp up
    { duration: '30s', target: 50 },   // Sustained load
    { duration: '10s', target: 100 },  // Peak
    { duration: '10s', target: 0 },    // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500'],   // 95th percentile < 500ms
    errors: ['rate<0.01'],              // Error rate < 1%
    health_latency: ['p(99)<100'],      // Health check < 100ms at p99
  },
};

export default function () {
  // ── Health check ───────────────────────────────────────────────
  const healthRes = http.get(`${BASE_URL}/health`);
  healthLatency.add(healthRes.timings.duration);
  check(healthRes, {
    'health: status 200': (r) => r.status === 200,
    'health: body has status': (r) => JSON.parse(r.body).status === 'ok',
  }) || errorRate.add(1);

  // ── Auth login ─────────────────────────────────────────────────
  if (API_KEY) {
    const loginRes = http.post(
      `${BASE_URL}/api/v1/auth/login`,
      JSON.stringify({ api_key: API_KEY }),
      { headers: { 'Content-Type': 'application/json' } }
    );
    authLatency.add(loginRes.timings.duration);
    
    if (loginRes.status === 200) {
      const token = JSON.parse(loginRes.body).token;

      // ── List devices (authenticated) ─────────────────────────
      const devicesRes = http.get(`${BASE_URL}/api/v1/devices`, {
        headers: { Authorization: `Bearer ${token}` },
      });
      devicesLatency.add(devicesRes.timings.duration);
      check(devicesRes, {
        'devices: status 200': (r) => r.status === 200,
        'devices: has array': (r) => Array.isArray(JSON.parse(r.body).devices),
      }) || errorRate.add(1);
    }
  }

  sleep(0.1);
}

export function handleSummary(data) {
  return {
    stdout: JSON.stringify(data, null, 2),
  };
}
