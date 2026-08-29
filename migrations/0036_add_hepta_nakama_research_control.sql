-- Durable Hepta -> Nakama signed research-control commands.
--
-- The exact signed request bytes are retained so a timeout, process death, or
-- expired short-lived control claim can only be retried byte-for-byte.  The
-- user-facing idempotency key is scoped by the fixed control operation while
-- command_id remains globally unique across every operation and session.

create table if not exists hepta_nakama_research_control_commands (
    command_id uuid primary key,
    operation text not null check (
        operation in ('create', 'resume', 'replace_roster', 'complete')
    ),
    target_rpc text not null check (
        target_rpc in (
            'trnm_research_session_create_v2',
            'trnm_research_session_resume_v2',
            'trnm_research_session_replace_roster_v2',
            'trnm_research_session_complete_v2'
        )
    ),
    idempotency_key text not null,
    request_hash text not null check (request_hash ~ '^sha256:[0-9a-f]{64}$'),
    session_id text not null,
    session_roster_version bigint not null check (
        session_roster_version between 1 and 9007199254740991
    ),
    authorization_set_id uuid not null
        references hepta_research_session_authorization_sets(authorization_set_id),
    payload_hash text not null check (payload_hash ~ '^sha256:[0-9a-f]{64}$'),
    request_body bytea not null,
    request_sha256 text not null check (request_sha256 ~ '^sha256:[0-9a-f]{64}$'),
    status text not null check (status in ('pending', 'applied')),
    response_body bytea,
    response_sha256 text check (
        response_sha256 is null or response_sha256 ~ '^sha256:[0-9a-f]{64}$'
    ),
    response_seal_signature text,
    attempt_count bigint not null default 0 check (
        attempt_count between 0 and 9007199254740991
    ),
    last_error_code text,
    record_json jsonb not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    unique (operation, idempotency_key),
    check (
        (status = 'pending' and response_body is null and response_sha256 is null
            and response_seal_signature is null)
        or
        (status = 'applied' and response_body is not null and response_sha256 is not null
            and response_seal_signature is not null)
    )
);

-- Keep repeated local migration runs forward-compatible with an early v2
-- development table created before the response seal was added.
alter table hepta_nakama_research_control_commands
    add column if not exists response_seal_signature text;

do $$
begin
    if not exists (
        select 1 from pg_constraint
        where conname = 'hepta_nakama_control_response_seal_required'
          and conrelid = 'hepta_nakama_research_control_commands'::regclass
    ) then
        alter table hepta_nakama_research_control_commands
            add constraint hepta_nakama_control_response_seal_required check (
                (status = 'pending' and response_seal_signature is null)
                or (status = 'applied' and response_seal_signature is not null)
            );
    end if;
end
$$;

create index if not exists hepta_nakama_research_control_session_idx
    on hepta_nakama_research_control_commands (
        session_id, session_roster_version, created_at
    );
