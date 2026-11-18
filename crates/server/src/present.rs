//! Turning service results into API shapes, with what each viewer may see (ACC-015, ACC-016).

use taskmaster_api::access as api;
use taskmaster_api::accounts::{
    Account, AccountSummary, DisabledLabel, DisabledState, PasswordState, ProfileTier, UserRef,
    UserRefKind,
};
use taskmaster_api::content::{IssueMention, TextRefs};
use taskmaster_api::files::{self as api_files, FileKind, FileRef};
use taskmaster_api::projects::ProjectRef;
use taskmaster_api::rules;
use taskmaster_domain::access::Access;
use taskmaster_domain::access::group::BuiltinGroup;
use taskmaster_domain::access::identity::{Tier, tier};
use taskmaster_domain::access::permission::{Entity, Operation, Permission, PermissionSet};
use taskmaster_domain::access::role::PredefinedRole;
use taskmaster_domain::accounts;
use taskmaster_domain::files::{Kind, ui_link_expiry};
use taskmaster_service::attachments::AttachmentView;
use taskmaster_service::files::Signer;
use taskmaster_service::people::{Names, Shown};
use taskmaster_service::pictures;
use taskmaster_service::text::Refs;
use taskmaster_store::attachments::Attachment;
use taskmaster_store::groups::Group;
use taskmaster_store::projects::Project;
use taskmaster_store::roles::Role;
use taskmaster_store::uploads::Upload;
use taskmaster_store::users::User;

/// What the project's rules said, as the answer carries it (AUT-023, step 1.9).
pub fn rule_messages(messages: &[taskmaster_service::rules::Message]) -> Vec<rules::Message> {
    messages
        .iter()
        .map(|message| rules::Message {
            rule: message.rule.key().to_owned(),
            code: message.code.to_owned(),
            params: message.params.clone().into_iter().collect(),
        })
        .collect()
}

/// A person as the viewer may see them.
pub fn user_ref(names: &Names, viewer: &Access, user: &User) -> UserRef {
    let person = names.show(viewer, user);
    let (kind, pseudonym) = match person.shown {
        Shown::Named => (UserRefKind::Person, None),
        Shown::Masked(number) => (UserRefKind::Masked, Some(number)),
        Shown::Deleted => (UserRefKind::Deleted, None),
    };
    UserRef {
        id: person.id,
        username: person.username,
        full_name: person.full_name,
        avatar_color: person.avatar_color,
        avatar_url: person
            .avatar_key
            .as_deref()
            .map(|key| avatar_url(person.id, key)),
        kind,
        pseudonym,
        disabled: person.disabled,
    }
}

/// A project as a reference.
pub fn project_ref(project: &Project) -> ProjectRef {
    ProjectRef {
        id: project.id,
        key: project.key.clone(),
        name: project.name.clone(),
        logo_color: project.logo_color,
        logo_url: logo_url(project),
        archived: project.is_archived(),
    }
}

/// An account in the directory, with the fields the viewer's tier allows.
pub fn account_summary(
    viewer: &Access,
    user: &User,
    groups: &[Group],
    projects: &[Project],
) -> AccountSummary {
    let tier = tier(viewer, user.id);
    let details = tier >= Tier::Details;
    AccountSummary {
        id: user.id,
        username: user.username.clone(),
        full_name: user.full_name.clone(),
        avatar_color: user.avatar_color,
        avatar_url: user
            .avatar_key
            .as_deref()
            .map(|key| avatar_url(user.id, key)),
        email: if details { user.email.clone() } else { None },
        email_verified: details && user.email_verified_at.is_some(),
        guest: user.is_guest(),
        invited: user.invited_at.is_some(),
        disabled: user.disabled_at.map(|at| DisabledState {
            at,
            reason: if details {
                user.disabled_reason.clone()
            } else {
                None
            },
            label: disabled_label(user.disabled_label.as_deref()),
        }),
        groups: if details {
            groups
                .iter()
                .filter(|group| group.builtin.is_none() && !group.is_team())
                .map(group_ref)
                .collect()
        } else {
            Vec::new()
        },
        projects: if details {
            projects.iter().map(project_ref).collect()
        } else {
            Vec::new()
        },
        full_profile: tier == Tier::Full,
        version: user.version,
        created_at: user.created_at,
    }
}

/// A profile (ACC-015).
pub fn account(viewer: &Access, profile: &taskmaster_service::users::Profile) -> Account {
    let groups = profile.groups.as_deref().unwrap_or_default();
    Account {
        summary: account_summary(viewer, &profile.user, groups, &profile.projects),
        tier: match profile.tier {
            Tier::Full => ProfileTier::Full,
            Tier::Details => ProfileTier::Details,
            Tier::Basic | Tier::Hidden => ProfileTier::Basic,
        },
        password: profile.password.map(|state| PasswordState {
            has_password: state.has_password,
            must_change: state.must_change,
        }),
    }
}

fn disabled_label(stored: Option<&str>) -> DisabledLabel {
    match stored.and_then(accounts::DisabledLabel::parse) {
        Some(accounts::DisabledLabel::Suspended) => DisabledLabel::Suspended,
        Some(accounts::DisabledLabel::Left) => DisabledLabel::Left,
        Some(accounts::DisabledLabel::Blocked) => DisabledLabel::Blocked,
        Some(accounts::DisabledLabel::Disabled) | None => DisabledLabel::Disabled,
    }
}

pub fn domain_label(label: DisabledLabel) -> accounts::DisabledLabel {
    match label {
        DisabledLabel::Disabled => accounts::DisabledLabel::Disabled,
        DisabledLabel::Suspended => accounts::DisabledLabel::Suspended,
        DisabledLabel::Left => accounts::DisabledLabel::Left,
        DisabledLabel::Blocked => accounts::DisabledLabel::Blocked,
    }
}

pub fn builtin_group(key: Option<&str>) -> Option<api::BuiltinGroupKey> {
    match key.and_then(BuiltinGroup::parse)? {
        BuiltinGroup::Everyone => Some(api::BuiltinGroupKey::Everyone),
        BuiltinGroup::AllMembers => Some(api::BuiltinGroupKey::AllMembers),
    }
}

pub fn group_ref(group: &Group) -> api::GroupRef {
    api::GroupRef {
        id: group.id,
        name: group.name.clone(),
        builtin: builtin_group(group.builtin.as_deref()),
        team_of: group.team_of,
    }
}

pub fn predefined(key: Option<&str>) -> Option<api::PredefinedRoleKey> {
    Some(match key.and_then(PredefinedRole::parse)? {
        PredefinedRole::SystemAdmin => api::PredefinedRoleKey::SystemAdmin,
        PredefinedRole::ProjectAdmin => api::PredefinedRoleKey::ProjectAdmin,
        PredefinedRole::ProjectMember => api::PredefinedRoleKey::ProjectMember,
        PredefinedRole::DirectoryViewer => api::PredefinedRoleKey::DirectoryViewer,
        PredefinedRole::ProjectCreator => api::PredefinedRoleKey::ProjectCreator,
        PredefinedRole::AccountCreator => api::PredefinedRoleKey::AccountCreator,
    })
}

pub fn role_ref(role: &Role) -> api::RoleRef {
    api::RoleRef {
        id: role.id,
        name: role.name.clone(),
        builtin: predefined(role.builtin.as_deref()),
    }
}

/// The API key of a permission. The two enums list the same keys (checked by a test).
pub fn permission_key(permission: Permission) -> Option<api::PermissionKey> {
    api::PermissionKey::parse(permission.key())
}

pub fn permission(key: api::PermissionKey) -> Option<Permission> {
    Permission::parse(&key.key())
}

pub fn permission_keys(set: PermissionSet) -> Vec<api::PermissionKey> {
    set.iter().filter_map(permission_key).collect()
}

pub fn permission_info(permission: Permission) -> Option<api::PermissionInfo> {
    Some(api::PermissionInfo {
        key: permission_key(permission)?,
        entity: match permission.entity() {
            Entity::System => api::PermissionEntity::System,
            Entity::User => api::PermissionEntity::User,
            Entity::Project => api::PermissionEntity::Project,
            Entity::Issue => api::PermissionEntity::Issue,
            Entity::Comment => api::PermissionEntity::Comment,
            Entity::Attachment => api::PermissionEntity::Attachment,
            Entity::Visibility => api::PermissionEntity::Visibility,
        },
        operation: match permission.operation() {
            Operation::Read => api::PermissionOperation::Read,
            Operation::Create => api::PermissionOperation::Create,
            Operation::Update => api::PermissionOperation::Update,
            Operation::Delete => api::PermissionOperation::Delete,
            Operation::Link => api::PermissionOperation::Link,
        },
        global_only: permission.global_only(),
        implies: permission_keys(permission.implies()),
        dependents: permission_keys(permission.dependents()),
    })
}

pub fn role(row: &taskmaster_service::roles::RoleRow) -> api::Role {
    api::Role {
        id: row.role.id,
        name: row.role.name.clone(),
        description: row.role.description.clone(),
        builtin: predefined(row.role.builtin.as_deref()),
        permissions: permission_keys(row.permissions),
        assignments: row.assignments,
        version: row.role.version,
        created_at: row.role.created_at,
    }
}

/// What rendered texts refer to, for the viewer (D-031): the people, issues and files among
/// `refs` that these texts name. Pass every mention, token and file name of the texts the answer
/// carries.
pub fn text_refs(
    names: &Names,
    viewer: &Access,
    signer: &Signer,
    refs: &Refs,
    mentioned: &[i64],
    tokens: &[String],
    file_names: &[String],
) -> TextRefs {
    let mut users: Vec<UserRef> = Vec::new();
    for user in refs.users.iter().filter(|u| mentioned.contains(&u.id)) {
        if !users.iter().any(|u| u.id == user.id) {
            users.push(user_ref(names, viewer, user));
        }
    }
    let mut issues: Vec<IssueMention> = Vec::new();
    for found in refs.issues.iter().filter(|i| tokens.contains(&i.token)) {
        if !issues.iter().any(|i| i.token == found.token) {
            issues.push(IssueMention {
                token: found.token.clone(),
                id: found.issue.id,
                readable_id: found.readable_id(),
                summary: found.issue.summary.clone(),
                resolved: found.issue.resolved_at.is_some(),
            });
        }
    }
    let links = FileLinks::new(signer, viewer, &refs.link_secret);
    let mut files: Vec<FileRef> = Vec::new();
    for name in file_names {
        if files.iter().any(|f| &f.name == name) {
            continue;
        }
        // An attached file wins over an upload of the same name.
        if let Some(file) = refs.files.iter().find(|f| &f.name == name) {
            let (url, thumbnail_url) = links.urls(file);
            files.push(FileRef {
                name: name.clone(),
                kind: file_kind(&file.content_type),
                size: file.size,
                width: file.width,
                height: file.height,
                url,
                thumbnail_url,
            });
        } else if let Some(upload) = refs.uploads.iter().find(|u| &u.name == name) {
            files.push(FileRef {
                name: name.clone(),
                kind: file_kind(&upload.content_type),
                size: upload.size,
                width: None,
                height: None,
                url: upload_url(upload),
                thumbnail_url: None,
            });
        }
    }
    TextRefs {
        users,
        issues,
        files,
    }
}

/// A path segment with everything but unreserved characters percent-encoded.
pub fn encode_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Where an uploaded avatar is: the key's last part, so a new picture gets a new URL.
pub fn avatar_url(user: i64, key: &str) -> String {
    format!("/api/v1/users/{user}/avatar/{}", pictures::file_of(key))
}

/// Where a project's uploaded logo is.
pub fn logo_url(project: &Project) -> Option<String> {
    project.logo_key.as_deref().map(|key| {
        format!(
            "/api/v1/projects/{}/logo/{}",
            project.id,
            pictures::file_of(key)
        )
    })
}

/// Where a staged upload is, for its sender.
pub fn upload_url(upload: &Upload) -> String {
    format!(
        "/api/v1/uploads/{}/{}",
        upload.id,
        encode_segment(&upload.name)
    )
}

pub fn file_kind(content_type: &str) -> FileKind {
    let kind = Kind::of_content_type(content_type);
    if kind.is_image() {
        FileKind::Image
    } else if kind.is_video() {
        FileKind::Video
    } else if kind == Kind::Pdf {
        FileKind::Pdf
    } else {
        FileKind::Other
    }
}

/// Signs the file links of one answer for its viewer (CNT-026).
pub struct FileLinks<'a> {
    signer: &'a Signer,
    viewer: i64,
    secret: &'a [u8],
    expires: time::OffsetDateTime,
}

impl<'a> FileLinks<'a> {
    pub fn new(signer: &'a Signer, viewer: &Access, secret: &'a [u8]) -> Self {
        Self {
            signer,
            viewer: viewer.user(),
            secret,
            expires: ui_link_expiry(time::OffsetDateTime::now_utc()),
        }
    }

    /// The file's URL and its thumbnail's.
    pub fn urls(&self, file: &Attachment) -> (String, Option<String>) {
        let link = self
            .signer
            .link(self.viewer, self.secret, file, self.expires);
        let base = format!(
            "/api/v1/files/{}/{}?u={}&e={}",
            file.id,
            encode_segment(&file.name),
            link.user,
            link.expires
        );
        let thumbnail = link
            .thumbnail_signature
            .map(|s| format!("{base}&thumb=1&s={s}"));
        (format!("{base}&s={}", link.signature), thumbnail)
    }
}

/// A file as the viewer sees it.
pub fn attachment<'a>(
    names: &Names,
    viewer: &Access,
    links: &FileLinks<'_>,
    view: &AttachmentView,
    user: &dyn Fn(i64) -> Option<&'a User>,
    group: &dyn Fn(i64) -> Option<&'a Group>,
) -> api_files::Attachment {
    let f = &view.file;
    let (url, thumbnail_url) = links.urls(f);
    let person = |id| user(id).map(|u| user_ref(names, viewer, u));
    api_files::Attachment {
        id: f.id,
        issue: f.issue_id,
        comment: f.comment_id,
        name: f.name.clone(),
        size: f.size,
        content_type: f.content_type.clone(),
        kind: file_kind(&f.content_type),
        width: f.width,
        height: f.height,
        url,
        thumbnail_url,
        author: person(f.author_id).unwrap_or_else(|| crate::issues::placeholder(f.author_id)),
        created_at: f.created_at,
        audience: taskmaster_api::issues::Audience {
            users: f
                .audience_users
                .iter()
                .filter_map(|id| person(*id))
                .collect(),
            groups: f
                .audience_groups
                .iter()
                .filter_map(|id| group(*id))
                .map(group_ref)
                .collect(),
        },
        can: api_files::AttachmentAbilities {
            update: view.can.update,
            delete: view.can.delete,
        },
        version: f.version,
    }
}

/// A staged upload, for its sender.
pub fn upload(upload: &Upload) -> api_files::Upload {
    api_files::Upload {
        id: upload.id,
        name: upload.name.clone(),
        size: upload.size,
        content_type: upload.content_type.clone(),
        kind: file_kind(&upload.content_type),
        url: upload_url(upload),
        created_at: upload.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_api_lists_every_permission() {
        for permission in Permission::ALL {
            let key = permission_key(permission).unwrap_or_else(|| panic!("{permission}"));
            assert_eq!(key.key(), permission.key());
            assert_eq!(super::permission(key), Some(permission));
        }
    }
}
