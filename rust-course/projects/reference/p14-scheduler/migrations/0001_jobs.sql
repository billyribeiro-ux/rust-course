CREATE TABLE jobs (
 id TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 80),
 payload TEXT NOT NULL CHECK(length(payload) BETWEEN 1 AND 1000),
 status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','done','cancelled','dead')),
 attempts INTEGER NOT NULL DEFAULT 0,
 token BIGINT NOT NULL DEFAULT 0,
 due BIGINT NOT NULL DEFAULT 0,
 lease_until BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX ready_jobs ON jobs(status,due,lease_until);
CREATE TABLE job_effects (job_id TEXT PRIMARY KEY REFERENCES jobs(id), payload TEXT NOT NULL);
