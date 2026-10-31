-- migration: saved_queries
-- created at: 20261030075755

CREATE TABLE "saved_queries" (
    "id" UUID PRIMARY KEY,
    "owner" TEXT,
    "profile_id" UUID,
    "name" TEXT NOT NULL,
    "sql" TEXT NOT NULL,
    "created_at" TIMESTAMPTZ NOT NULL,
    "updated_at" TIMESTAMPTZ NOT NULL
);

CREATE INDEX "saved_queries_owner_updated_at_id_idx"
    ON "saved_queries" ("owner", "updated_at" DESC, "id" DESC);

CREATE INDEX "saved_queries_owner_profile_updated_at_id_idx"
    ON "saved_queries" ("owner", "profile_id", "updated_at" DESC, "id" DESC);