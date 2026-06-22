import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';

// Reproducible business-path smoke test for internal/staging SLO validation.
// Target: intent-api:8080 (internal ClusterIP, no public ingress).
// This is a bounded non-production load test. Do not claim production readiness.

export const options = {
  stages: [
    { duration: '1m', target: 5 },   // Ramp up to 5 VUs
    { duration: '5m', target: 5 },  // Sustain 5 VUs
    { duration: '1m', target: 0 },  // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<100'], // p95 < 100ms (internal SLO target)
    http_req_failed: ['rate<0.001'],  // error rate < 0.1%
  },
};

const BASE_URL = __ENV.API_BASE_URL || 'http://intent-api:8080';
const API_KEY = __ENV.API_KEY || 'test-key-not-real';

const healthChecks = new Counter('health_checks');
const healthFailures = new Counter('health_failures');
const createChecks = new Counter('create_checks');
const createFailures = new Counter('create_failures');
const errorRate = new Rate('errors');
const latencyTrend = new Trend('latency');

export default function () {
  const headers = {
    'X-Api-Key': API_KEY,
    'Content-Type': 'application/json',
  };

  // 1. Health check (smoke)
  const healthRes = http.get(`${BASE_URL}/health`, { headers });
  const healthOk = check(healthRes, {
    'health status is 200': (r) => r.status === 200,
    'health response time < 100ms': (r) => r.timings.duration < 100,
  });
  healthChecks.add(1);
  if (!healthOk) healthFailures.add(1);
  errorRate.add(healthRes.status !== 200);
  latencyTrend.add(healthRes.timings.duration);

  // 2. Ready check (smoke)
  const readyRes = http.get(`${BASE_URL}/ready`, { headers });
  check(readyRes, {
    'ready status is 200': (r) => r.status === 200,
  });
  errorRate.add(readyRes.status !== 200);
  latencyTrend.add(readyRes.timings.duration);

  // 3. List intents (read, touches DB if auth passes)
  const listRes = http.get(`${BASE_URL}/v1/intents`, { headers });
  check(listRes, {
    'list intents returns 200 or 401': (r) => r.status === 200 || r.status === 401,
  });
  errorRate.add(listRes.status >= 500);
  latencyTrend.add(listRes.timings.duration);

  // 4. Create intent (write, DB-backed if auth passes)
  const payload = JSON.stringify({
    name: `load-test-${__VU}-${__ITER}-${Date.now()}`,
    tenant_id: '00000000-0000-0000-0000-000000000001',
  });
  const createRes = http.post(`${BASE_URL}/v1/intents`, payload, { headers });
  const createOk = check(createRes, {
    'create intent returns 201 or 401': (r) => r.status === 201 || r.status === 401,
  });
  createChecks.add(1);
  if (!createOk) createFailures.add(1);
  errorRate.add(createRes.status >= 500);
  latencyTrend.add(createRes.timings.duration);

  sleep(1);
}

export function handleSummary(data) {
  return {
    stdout: JSON.stringify({
      total_requests: data.metrics.http_reqs.count,
      failed_requests: data.metrics.http_req_failed.count,
      error_rate: data.metrics.http_req_failed.rate,
      p95_latency_ms: data.metrics.http_req_duration['p(95)'],
      p99_latency_ms: data.metrics.http_req_duration['p(99)'],
      avg_latency_ms: data.metrics.http_req_duration.avg,
      health_checks: data.metrics.health_checks ? data.metrics.health_checks.count : 0,
      health_failures: data.metrics.health_failures ? data.metrics.health_failures.count : 0,
      create_checks: data.metrics.create_checks ? data.metrics.create_checks.count : 0,
      create_failures: data.metrics.create_failures ? data.metrics.create_failures.count : 0,
      thresholds_passed: Object.values(data.thresholds).every(t => t.ok),
      vus_max: data.metrics.vus_max ? data.metrics.vus_max.max : 0,
      duration_ms: data.state.testRunDurationMs,
    }),
  };
}
