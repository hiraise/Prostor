//! Which topics of a `subscribe` a person may have (architecture.md §5): one query for all its
//! `issue:` topics, the rest from their access.

use std::collections::HashSet;

use prostor_domain::access::Access;
use prostor_domain::access::permission::Permission;
use prostor_domain::live::{Guard, ListScope, Topic};
use prostor_domain::{BoardId, IssueId, ProjectId};
use prostor_store::PgPool;

/// The accepted topics and the denied strings, each in the order asked, without repeats.
pub struct Checked {
    pub accepted: Vec<Topic>,
    pub denied: Vec<String>,
    /// The projects of each accepted board: the pump routes their issue events (BRD-114).
    pub boards: Vec<(BoardId, Vec<ProjectId>)>,
}

pub async fn check(db: &PgPool, access: &Access, asked: &[String]) -> Result<Checked, sqlx::Error> {
    let mut seen = HashSet::new();
    let parsed: Vec<(&String, Option<Topic>)> = asked
        .iter()
        .filter(|raw| seen.insert(raw.as_str()))
        .map(|raw| (raw, Topic::parse(raw)))
        .collect();
    let ids: Vec<IssueId> = parsed
        .iter()
        .filter_map(|(_, topic)| match topic {
            Some(Topic::Issue(id)) => Some(*id),
            _ => None,
        })
        .collect();
    let visible: HashSet<IssueId> = if ids.is_empty() {
        HashSet::new()
    } else {
        prostor_store::live::issue_facts(db, &ids)
            .await?
            .into_iter()
            .filter(|facts| {
                !facts.draft && !facts.deleted && Guard::issue(&facts.rule()).allows(access)
            })
            .map(|facts| facts.id)
            .collect()
    };
    let board_ids: Vec<BoardId> = parsed
        .iter()
        .filter_map(|(_, topic)| match topic {
            Some(Topic::Board(id)) => Some(*id),
            _ => None,
        })
        .collect();
    let boards: Vec<(BoardId, Vec<ProjectId>)> = if board_ids.is_empty() {
        Vec::new()
    } else {
        prostor_store::boards::get_many(db, &board_ids)
            .await?
            .into_iter()
            .filter(|b| b.access().can_view(access))
            .map(|b| (b.id, b.projects))
            .collect()
    };
    let mut accepted = Vec::new();
    let mut denied = Vec::new();
    for (raw, topic) in parsed {
        let allowed = match &topic {
            Some(Topic::Issue(id)) => visible.contains(id),
            Some(Topic::Board(id)) => boards.iter().any(|(b, _)| b == id),
            // Only the reader's own inbox (plan step 1.10 §3.6).
            Some(Topic::Inbox(id)) => *id == access.user(),
            Some(Topic::Instance | Topic::List(ListScope::All)) => true,
            // Every listed project must be one whose issues they may list.
            Some(Topic::List(ListScope::Projects(ids))) => ids.iter().all(|project| {
                access.has_in(Permission::IssueRead, *project)
                    || access.has_in(Permission::IssueCreate, *project)
            }),
            None => false,
        };
        match topic {
            Some(topic) if allowed => accepted.push(topic),
            _ => denied.push(raw.clone()),
        }
    }
    Ok(Checked {
        accepted,
        denied,
        boards,
    })
}
