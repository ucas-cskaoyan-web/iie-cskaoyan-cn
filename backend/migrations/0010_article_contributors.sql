ALTER TABLE articles
    ADD COLUMN IF NOT EXISTS contributor_id UUID REFERENCES contributors(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS articles_contributor_idx ON articles (contributor_id);
