CREATE TABLE issues (
 id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 team_id BIGINT NOT NULL CHECK (team_id > 0),
 title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
 closed BOOLEAN NOT NULL DEFAULT false,
 revision BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX issues_team_cursor ON issues(team_id,id);
CREATE TABLE issue_audit (
 issue_id BIGINT NOT NULL REFERENCES issues(id),
 revision BIGINT NOT NULL,
 actor BIGINT NOT NULL,
 action TEXT NOT NULL,
 PRIMARY KEY(issue_id,revision)
);
