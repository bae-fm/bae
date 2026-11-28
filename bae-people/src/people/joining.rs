//! The write that gives a person their profile as each of their devices
//! registers (§5.4).

use bae_store::WriteStatements;

// The private profile of a member who has none.
coven::write!(
    CreateJoiningProfile(member: coven::MemberId, name: &'p str, circle: coven::CircleId),
    r#"
    INSERT INTO people (id, name, private_circle, history_sharing)
    VALUES (:member, :name, :circle, 'private')
    ON CONFLICT (id) DO NOTHING
"#
);

// A member's profile points at `circle`, where it names another.
coven::write!(
    MoveProfileCircle(member: coven::MemberId, circle: coven::CircleId),
    r#"
    UPDATE people SET private_circle = :circle
    WHERE id = :member AND private_circle <> :circle
"#
);

/// The name of `member`'s private circle: the circle a join's request asks
/// for, or the one a library's creation makes for its creator, which their
/// profile's `private_circle` names.
pub fn private_circle_name(member: &coven::MemberId) -> String {
    format!("bae-person:{member}")
}

/// The person's profile, written as coven first applies the registration of
/// one of their devices (the creation of the library, a join, a restore, a
/// pairing, a reset, or the install of a snapshot that covers it), in the same
/// transaction, from the facts of their latest admission, or of the library's
/// first slot for its creator (§5.4).
///
/// A member with no profile and facts that carry a name gets a private
/// profile under that name, in the circle those facts name. A recovery
/// carries no name: the member's profile is already there, and its
/// `private_circle` is pointed at the circle the recovery made when it names
/// another, since the old circle's key is lost with the old keys. Nothing
/// else is written, so a member's second device, and a retry, write nothing.
pub(crate) fn write_profile(
    sql: &coven::JoiningWrite<'_>,
    joining: &coven::Joining,
) -> Result<(), coven::DbError> {
    match &joining.person {
        Some(name) => {
            sql.run(&CreateJoiningProfileParams {
                member: joining.member,
                name,
                circle: joining.circle,
            })?;
        }
        None => {
            sql.run(&MoveProfileCircleParams { member: joining.member, circle: joining.circle })?;
        }
    }
    Ok(())
}
