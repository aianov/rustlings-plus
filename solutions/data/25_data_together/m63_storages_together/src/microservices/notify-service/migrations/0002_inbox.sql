CREATE TABLE inbox (
    key TEXT PRIMARY KEY,
    at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX inbox_by_time ON inbox (at);
