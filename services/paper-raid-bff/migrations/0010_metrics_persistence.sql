-- Durable aggregate metrics for the Paper Raid BFF.
--
-- The Prometheus endpoint is intentionally loopback-only, but an in-process
-- counter alone disappears on restart and cannot support the P0 recovery
-- drill.  This table stores one bounded, content-free aggregate snapshot;
-- product/event detail remains in paper_raid_bff_product_events.

CREATE TABLE IF NOT EXISTS paper_raid_bff_metrics_snapshots (
    snapshot_name TEXT PRIMARY KEY
        CHECK (snapshot_name = 'paper_raid_bff'),
    snapshot_schema TEXT NOT NULL
        CHECK (snapshot_schema = 'hepta.paper_raid.metrics_snapshot.v1'),
    snapshot_revision BIGINT NOT NULL CHECK (snapshot_revision >= 0),
    snapshot_json JSONB NOT NULL
        CHECK (jsonb_typeof(snapshot_json) = 'object'),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS paper_raid_bff_metrics_snapshots_updated_idx
    ON paper_raid_bff_metrics_snapshots(updated_at DESC);
