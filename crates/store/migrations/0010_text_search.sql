-- Full-text search (step 1.5c, D-037): one configuration for English and Russian words in one
-- index, stored documents of every searched text, and indexes for mention search.

-- Latin words take English stems, Cyrillic words (parser token `word`) Russian stems; numbers,
-- e-mails, URLs and the rest stay as written. Each dictionary brings its stop words.
create text search configuration trackflow_text (copy = simple);
alter text search configuration trackflow_text
    alter mapping for asciiword, asciihword, hword_asciipart with english_stem;
alter text search configuration trackflow_text
    alter mapping for word, hword, hword_part with russian_stem;

-- The configuration is named, so `to_tsvector` is immutable and can make generated columns.
-- Adding them rewrites the tables once.
alter table issues
    add column summary_doc tsvector
        generated always as (to_tsvector('trackflow_text', summary)) stored,
    add column description_doc tsvector
        generated always as (to_tsvector('trackflow_text', coalesce(description, ''))) stored;
create index issues_summary_doc_idx on issues using gin (summary_doc)
    where number is not null and deleted_at is null;
create index issues_description_doc_idx on issues using gin (description_doc)
    where number is not null and deleted_at is null;

alter table issue_text_values
    add column doc tsvector generated always as (to_tsvector('trackflow_text', value)) stored;
create index issue_text_values_doc_idx on issue_text_values using gin (doc);

alter table comments
    add column doc tsvector generated always as (to_tsvector('trackflow_text', text)) stored;
create index comments_doc_idx on comments using gin (doc) where deleted_at is null;

-- Mentions of issues (CNT-058); mentions of people have their indexes since 0006.
create index issues_issue_tokens_idx on issues using gin (issue_tokens)
    where cardinality(issue_tokens) > 0;
create index comments_issue_tokens_idx on comments using gin (issue_tokens)
    where cardinality(issue_tokens) > 0;
