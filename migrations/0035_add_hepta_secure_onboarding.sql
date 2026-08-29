-- Replay ledger for Agent-owned Paper Raid binding proofs.
--
-- The legacy v1 Agent registry remains compatible, but its self-asserted
-- owner_id is never consulted by the v2 Paper Raid authorization path.

create table if not exists hepta_agent_binding_nonces (
    agent_id text not null,
    nonce text not null,
    binding_id uuid not null references hepta_agent_bindings(binding_id) on delete cascade,
    accepted_at timestamptz not null default now(),
    primary key (agent_id, nonce),
    unique (binding_id)
);

create table if not exists hepta_agent_binding_key_rotations (
    rotation_id uuid primary key,
    binding_id uuid not null references hepta_agent_bindings(binding_id),
    nonce text not null,
    expected_binding_version bigint not null check (
        expected_binding_version between 1 and 9007199254740991
    ),
    record_json jsonb not null,
    accepted_at timestamptz not null default now(),
    unique (binding_id, nonce)
);
