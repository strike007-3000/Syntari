-- Summaries become searchable by Ask across meetings. Rowid = sessions rowid, kept in step by triggers (never VACUUM in place).
CREATE VIRTUAL TABLE fts_summary USING fts5(text, tokenize = 'porter unicode61');
INSERT INTO fts_summary(rowid, text) SELECT rowid, summary FROM sessions WHERE summary IS NOT NULL AND summary != '';
CREATE TRIGGER summary_fts_au AFTER UPDATE OF summary ON sessions BEGIN
    DELETE FROM fts_summary WHERE rowid = old.rowid;
    INSERT INTO fts_summary(rowid, text) SELECT new.rowid, new.summary WHERE new.summary IS NOT NULL AND new.summary != '';
END;
CREATE TRIGGER summary_fts_ad AFTER DELETE ON sessions BEGIN
    DELETE FROM fts_summary WHERE rowid = old.rowid;
END;
