-- migration: query_history_owner_timeline
-- created at: 20261030093000

CREATE INDEX "query_history_owner_executed_at_id_idx"
    ON "query_history" ("owner", "executed_at" DESC, "id" DESC);
