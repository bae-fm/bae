//! What the membership domain offers to other reads: a member of the library
//! with their devices, and whether the member using this device is an admin,
//! both read from coven's views. The members list runs the read here, the
//! rating systems read spreads whether the member is an admin, and the people
//! reads spread the member.

sqlive_compose::fragment!(
    SelfIsAdmin { self_is_admin: bool },
    r#"
    SELECT COUNT(*) > 0 AS self_is_admin
    FROM coven_members AS s
    WHERE s.is_self AND NOT s.removed AND s.role = 'admin'
"#
);

sqlive_compose::fragment!(
    Member {
        member_id: coven::MemberId,
        removed: bool,
        role: coven::MemberRole,
        is_self: bool,
        account: Option<String>,
        access_key_id: Option<String>,
        struct ActiveDeviceDocument { id: hex, name: String, is_self: bool }
        active_devices: json<Vec<ActiveDeviceDocument>>,
        struct FormerDeviceDocument { id: hex, name: String, state: String }
        former_devices: json<Vec<FormerDeviceDocument>>,
        active_device_count: u32,
        former_device_count: u32,
    },
    r#"
    SELECT m.member AS member_id, m.removed, m.role, m.is_self, m.account, m.access_key_id,
           (SELECT json_group_array(json_object(
                       'id', hex(d.device), 'name', d.name,
                       'is_self', json(CASE WHEN d.is_self THEN 'true' ELSE 'false' END))
                       ORDER BY d.device)
            FROM coven_devices AS d
            WHERE d.member = m.member AND d.state = 'active') AS active_devices,
           (SELECT json_group_array(json_object(
                       'id', hex(d.device), 'name', d.name, 'state', d.state)
                       ORDER BY d.device)
            FROM coven_devices AS d
            WHERE d.member = m.member AND d.state <> 'active') AS former_devices,
           (SELECT COUNT(*) FROM coven_devices AS d
            WHERE d.member = m.member AND d.state = 'active') AS active_device_count,
           (SELECT COUNT(*) FROM coven_devices AS d
            WHERE d.member = m.member AND d.state <> 'active') AS former_device_count,
           viewer.*
    FROM coven_members AS m
    JOIN @crate::membership::fragments::SelfIsAdmin AS viewer ON 1
"#
);

// The members who remain in the library, in member-id order.
sqlive_compose::read!(
    writer CurrentMembers,
    r#"
    SELECT member.*
    FROM @crate::membership::fragments::Member AS member
    WHERE NOT member.removed
    ORDER BY member.member_id
"#
);
