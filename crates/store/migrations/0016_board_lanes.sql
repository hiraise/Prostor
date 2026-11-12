-- Lanes (step 1.8c, D-046): rows of a board by a field's values or by parent issues.
alter table boards
    add column lane_mode text not null default 'none'
        check (lane_mode in ('none', 'values', 'parents')),
    -- The field whose values make lanes (`values`) or limit the parents (`parents`).
    add column lane_field_id bigint references fields (id) on delete set null,
    -- An attribute instead of a field (`values` only).
    add column lane_attribute text check (lane_attribute in ('project', 'tags', 'created', 'updated')),
    -- Lane keys in order (`v:<value>`, `u:<account>`, `g:<group>`, `p:<project>`, `t:<tag>`,
    -- `d:<bucket>`); for `parents`, the value keys a parent must hold.
    add column lane_values text[] not null default '{}',
    add column other_lane text not null default 'bottom'
        check (other_lane in ('off', 'top', 'bottom')),
    -- The value new cards get on a parent-lane board (BRD-055).
    add column new_card_value_id bigint references field_values (id) on delete set null;

-- A row with a lane rank is a parent lane of its sprint, not a card (BRD-051, BRD-053).
alter table sprint_issues add column lane_rank text collate "C";
create index sprint_issues_lane_idx on sprint_issues (sprint_id, lane_rank)
    where lane_rank is not null;

-- The draft each cell's «+» keeps for its person (BRD-077): `{ "<sprint>:<column>:<lane>": issue }`.
alter table board_profiles add column cell_drafts jsonb not null default '{}';
