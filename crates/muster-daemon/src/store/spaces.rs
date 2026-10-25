//! The person's spaces and what is in them (`ARCHITECTURE.md` §2, §4).
//! A membership goes in the same transaction as what it names, as a
//! foreign key cannot point at both projects and sessions.

use muster_core::spaces::Member;
use muster_core::{ProjectId, SessionId, Space, SpaceId, Timestamp};
use rusqlite::{Connection, Row, params};

use super::rows::id_text;
use super::{Result, Store, StoreError};

/// How a member's kind is stored.
fn kind(member: Member) -> (&'static str, String) {
    match member {
        Member::Project(id) => ("project", id_text(id)),
        Member::Session(id) => ("session", id_text(id)),
    }
}

fn parse<T: std::str::FromStr>(text: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    text.parse()
        .map_err(|error: T::Err| StoreError::Invalid(error.to_string()))
}

fn member(row: &Row<'_>) -> rusqlite::Result<Result<(String, Member)>> {
    let (space, kind, id): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
    Ok((|| {
        let member = match kind.as_str() {
            "project" => Member::Project(parse(&id)?),
            "session" => Member::Session(parse(&id)?),
            other => return Err(StoreError::Invalid(format!("member kind {other:?}"))),
        };
        Ok((space, member))
    })())
}

impl Store {
    /// Every space, in the order they were made, each with what is in it
    /// in the order it was added.
    pub fn spaces(&self) -> Result<Vec<Space>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name, created_at FROM spaces ORDER BY created_at, id")?;
        let mut spaces = statement
            .query_map([], |row| {
                let (id, name, at): (String, String, i64) = (row.get(0)?, row.get(1)?, row.get(2)?);
                Ok((id, name, at))
            })?
            .map(|row| {
                let (id, name, at) = row?;
                Ok(Space {
                    id: parse(&id)?,
                    name,
                    members: Vec::new(),
                    created_at: Timestamp::from_millis(at),
                })
            })
            .collect::<Result<Vec<Space>>>()?;
        let mut statement = self
            .conn
            .prepare("SELECT space_id, kind, member_id FROM space_members ORDER BY rowid")?;
        for found in statement.query_map([], member)? {
            let (space, member) = found??;
            let space: SpaceId = parse(&space)?;
            if let Some(holder) = spaces.iter_mut().find(|found| found.id == space) {
                holder.members.push(member);
            }
        }
        Ok(spaces)
    }

    pub fn insert_space(&mut self, space: &Space) -> Result<()> {
        self.conn.execute(
            "INSERT INTO spaces (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id_text(space.id), space.name, space.created_at.as_millis()],
        )?;
        Ok(())
    }

    pub fn rename_space(&mut self, space: SpaceId, name: &str) -> Result<()> {
        match self.conn.execute(
            "UPDATE spaces SET name = ?2 WHERE id = ?1",
            params![id_text(space), name],
        )? {
            0 => Err(StoreError::NotFound(space.to_string())),
            _ => Ok(()),
        }
    }

    /// Removes a space and its memberships; what was in it stays.
    pub fn delete_space(&mut self, space: SpaceId) -> Result<()> {
        match self
            .conn
            .execute("DELETE FROM spaces WHERE id = ?1", [id_text(space)])?
        {
            0 => Err(StoreError::NotFound(space.to_string())),
            _ => Ok(()),
        }
    }

    /// Puts `member` in `space`; again does nothing.
    pub fn add_member(&mut self, space: SpaceId, member: Member) -> Result<()> {
        let (kind, id) = kind(member);
        self.conn.execute(
            "INSERT OR IGNORE INTO space_members (space_id, kind, member_id) VALUES (?1, ?2, ?3)",
            params![id_text(space), kind, id],
        )?;
        Ok(())
    }

    pub fn remove_member(&mut self, space: SpaceId, member: Member) -> Result<()> {
        let (kind, id) = kind(member);
        self.conn.execute(
            "DELETE FROM space_members WHERE space_id = ?1 AND kind = ?2 AND member_id = ?3",
            params![id_text(space), kind, id],
        )?;
        Ok(())
    }
}

/// Takes `session` out of every space.
pub(super) fn forget_session(conn: &Connection, session: SessionId) -> Result<()> {
    conn.execute(
        "DELETE FROM space_members WHERE kind = 'session' AND member_id = ?1",
        [id_text(session)],
    )?;
    Ok(())
}

/// Takes `project`, and every session in its workspaces, out of every space.
pub(super) fn forget_project(conn: &Connection, project: ProjectId) -> Result<()> {
    conn.execute(
        "DELETE FROM space_members
         WHERE (kind = 'project' AND member_id = ?1)
            OR (kind = 'session' AND member_id IN
                (SELECT s.id FROM sessions s JOIN workspaces w ON w.id = s.workspace_id
                 WHERE w.project_id = ?1))",
        [id_text(project)],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "spaces_tests.rs"]
mod tests;
