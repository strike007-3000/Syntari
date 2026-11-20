-- Full-text index for Ask across meetings. Each index row's rowid is the source row's rowid, kept in step by triggers.
-- (Never VACUUM the live database in place: that can renumber rowids. Back up with VACUUM INTO, as the app does.)
CREATE VIRTUAL TABLE fts_transcript USING fts5(text, tokenize = 'porter unicode61');
CREATE VIRTUAL TABLE fts_notes USING fts5(text, tokenize = 'porter unicode61');

INSERT INTO fts_transcript(rowid, text) SELECT rowid, text FROM transcript;
INSERT INTO fts_notes(rowid, text) SELECT rowid, COALESCE(expanded, raw) FROM notes;

CREATE TRIGGER transcript_fts_ai AFTER INSERT ON transcript BEGIN
    INSERT INTO fts_transcript(rowid, text) VALUES (new.rowid, new.text);
END;
CREATE TRIGGER transcript_fts_ad AFTER DELETE ON transcript BEGIN
    DELETE FROM fts_transcript WHERE rowid = old.rowid;
END;
CREATE TRIGGER transcript_fts_au AFTER UPDATE OF text ON transcript BEGIN
    DELETE FROM fts_transcript WHERE rowid = old.rowid;
    INSERT INTO fts_transcript(rowid, text) VALUES (new.rowid, new.text);
END;

CREATE TRIGGER notes_fts_ai AFTER INSERT ON notes BEGIN
    INSERT INTO fts_notes(rowid, text) VALUES (new.rowid, COALESCE(new.expanded, new.raw));
END;
CREATE TRIGGER notes_fts_ad AFTER DELETE ON notes BEGIN
    DELETE FROM fts_notes WHERE rowid = old.rowid;
END;
CREATE TRIGGER notes_fts_au AFTER UPDATE OF raw, expanded ON notes BEGIN
    DELETE FROM fts_notes WHERE rowid = old.rowid;
    INSERT INTO fts_notes(rowid, text) VALUES (new.rowid, COALESCE(new.expanded, new.raw));
END;
