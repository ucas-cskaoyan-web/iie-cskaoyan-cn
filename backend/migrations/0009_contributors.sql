CREATE TABLE IF NOT EXISTS contributors (
    id UUID PRIMARY KEY,
    nickname TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('qq', 'wechat', 'github')),
    account TEXT NOT NULL,
    avatar_url TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order BETWEEN 0 AND 10000),
    is_visible BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (platform, account)
);

CREATE INDEX IF NOT EXISTS contributors_public_idx
    ON contributors (is_visible, sort_order, created_at);

ALTER TABLE submissions
    ADD COLUMN IF NOT EXISTS contributor_platform TEXT,
    ADD COLUMN IF NOT EXISTS contributor_account TEXT,
    ADD COLUMN IF NOT EXISTS contributor_nickname TEXT,
    ADD COLUMN IF NOT EXISTS contributor_avatar_url TEXT;
