do $$
begin
    if exists (
        select 1
        from slack_installations
        group by slack_team_id, slack_app_id
        having count(*) > 1
    ) then
        raise exception
            'cannot enforce Slack workspace ownership: duplicate workspace and app installations exist';
    end if;
end
$$;

alter table slack_installations
add constraint slack_installations_team_app_unique
unique (slack_team_id, slack_app_id);
