-- migration: query_history
-- created at: 20261030075754

CREATE TABLE "query_history" (
    "id" UUID PRIMARY KEY,
    "owner" TEXT,
    "profile_id" UUID NOT NULL,
    "sql" TEXT NOT NULL,
    "executed_at" TIMESTAMPTZ NOT NULL,
    "elapsed_ms" BIGINT NOT NULL CHECK ("elapsed_ms" >= 0),
    "rows" BIGINT CHECK ("rows" >= 0),
    "rows_affected" BIGINT CHECK ("rows_affected" >= 0),
    "status" TEXT NOT NULL CHECK ("status" IN ('succeeded', 'failed', 'canceled')),
    "error_message" TEXT
);

CREATE INDEX "query_history_owner_profile_executed_at_id_idx"
    ON "query_history" ("owner", "profile_id", "executed_at" DESC, "id" DESC);