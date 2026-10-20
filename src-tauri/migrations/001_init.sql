CREATE TABLE sessions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER
);

CREATE TABLE notes (
    id              TEXT PRIMARY KEY,
    session_id      INTEGER REFERENCES sessions(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('decision','action','risk','discussion')),
    raw             TEXT NOT NULL,
    expanded        TEXT,
    owner           TEXT,
    due             TEXT,
    done            INTEGER NOT NULL DEFAULT 0,
    ts_ms           INTEGER NOT NULL,
    citation_quote  TEXT,
    citation_chunks TEXT
);
CREATE INDEX idx_notes_session ON notes(session_id, ts_ms);

CREATE TABLE transcript (
    session_id INTEGER REFERENCES sessions(id) ON DELETE CASCADE,
    chunk_id   INTEGER NOT NULL,
    speaker    TEXT NOT NULL CHECK (speaker IN ('You','Remote')),
    text       TEXT NOT NULL,
    ts_ms      INTEGER NOT NULL,
    PRIMARY KEY (session_id, chunk_id)
);
