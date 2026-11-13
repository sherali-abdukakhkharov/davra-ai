-- Agent usage from Claude Code's OpenTelemetry metrics (OTLP/HTTP JSON,
-- delta temporality), summed per project, member, local day and model.
-- Counters without a model (sessions, commits, active time) use model ''.
-- Only counts are stored: no prompts, code, emails or session ids.

create table agent_usage (
  org_id             uuid not null references orgs (id) on delete cascade,
  project_id         uuid not null references projects (id) on delete cascade,
  member_id          uuid not null references members (id) on delete cascade,
  day                date not null,
  model              text not null default '',
  cost_usd           double precision not null default 0,
  input_tokens       bigint not null default 0,
  output_tokens      bigint not null default 0,
  cache_read_tokens  bigint not null default 0,
  cache_write_tokens bigint not null default 0,
  sessions           bigint not null default 0,
  lines_added        bigint not null default 0,
  lines_removed      bigint not null default 0,
  commits            bigint not null default 0,
  pull_requests      bigint not null default 0,
  edits_accepted     bigint not null default 0,
  edits_rejected     bigint not null default 0,
  active_secs        double precision not null default 0,
  updated_at         timestamptz not null default now(),
  primary key (project_id, member_id, day, model)
);

create index agent_usage_project_day on agent_usage (project_id, day);

do $rls$
begin
  if exists (select 1 from pg_roles where rolname = 'davra_app') then
    grant select, insert, update on agent_usage to davra_app;
    alter table agent_usage enable row level security;
    create policy tenant on agent_usage to davra_app
      using (org_id = davra_org()) with check (org_id = davra_org());
  end if;
end
$rls$;
