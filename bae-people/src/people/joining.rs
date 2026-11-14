//! The write that gives a person their profile as each of their devices
//! registers (§5.4).

use rusqlite::params;

/// The name of `member`'s private circle: the circle a join's request asks
/// for, and the one their profile's `private_circle` names.
pub fn private_circle_name(member: &coven::MemberId) -> String {
    format!("bae-person:{member}")
}

/// The person's profile, written as coven first applies the registration of
/// one of their devices (a join, a restore, a pairing, a reset, or the install
/// of a snapshot that covers it), in the same transaction, from the facts of
/// their latest admission (§5.4).
///
/// A member with no profile and an admission that carried a name gets a
/// private profile under that name, in the circle the admission made. A
/// recovery carries no name: the member's profile is already there, and its
/// `private_circle` is pointed at the circle the recovery made when it names
/// another, since the old circle's key is lost with the old keys. Nothing
/// else is written, so a member's second device, and a retry, write nothing.
pub(crate) fn write_profile(
    sql: &coven::JoiningWrite<'_>,
    joining: &coven::Joining,
) -> Result<(), coven::DbError> {
    let member = joining.member.to_string();
    let circle = joining.circle.to_string();
    let existing =
        sql.query("SELECT private_circle FROM people WHERE id = ?1", [&member], |row| {
            row.get::<_, String>(0)
        })?;
    match (existing.first(), &joining.person) {
        (None, Some(name)) => {
            sql.execute(
                "INSERT INTO people (id, name, private_circle, history_sharing) \
                 VALUES (?1, ?2, ?3, 'private')",
                params![member, name, circle],
            )?;
        }
        (Some(current), None) if *current != circle => {
            sql.execute(
                "UPDATE people SET private_circle = ?2 WHERE id = ?1",
                params![member, circle],
            )?;
        }
        _ => {}
    }
    Ok(())
}
