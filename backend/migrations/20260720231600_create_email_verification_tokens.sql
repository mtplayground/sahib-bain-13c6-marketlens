CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_sub TEXT NOT NULL REFERENCES users(sub) ON DELETE CASCADE,
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT email_verification_tokens_email_not_blank CHECK (length(trim(email)) > 0),
    CONSTRAINT email_verification_tokens_hash_not_blank CHECK (length(trim(token_hash)) > 0)
);

CREATE INDEX IF NOT EXISTS email_verification_tokens_user_sub_idx
    ON email_verification_tokens (user_sub);

CREATE INDEX IF NOT EXISTS email_verification_tokens_expires_at_idx
    ON email_verification_tokens (expires_at);
