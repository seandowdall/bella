alter table organization_agent_llm_settings
    drop constraint organization_agent_llm_settings_pkey;

alter table organization_agent_llm_settings
    add column id uuid,
    add column display_name text not null default 'Default',
    add column is_default boolean not null default true;

update organization_agent_llm_settings
set id = gen_random_uuid()
where id is null;

alter table organization_agent_llm_settings
    alter column id set not null,
    add primary key (id),
    add check (char_length(display_name) between 1 and 80);

create unique index organization_agent_llm_settings_default_idx
on organization_agent_llm_settings(organization_id)
where is_default;

create unique index organization_agent_llm_settings_name_idx
on organization_agent_llm_settings(organization_id, display_name);
