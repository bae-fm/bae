use super::*;

#[test]
fn every_tool_input_schema_has_root_object_type() {
    for tool in AutomationTool::all() {
        let schema = tool.input_schema();
        assert_eq!(
            schema.get("type").and_then(Value::as_str),
            Some("object"),
            "tool {} inputSchema must have root type object",
            tool.name(),
        );
    }
}

/// The storage tool accepts and refuses what the Storage Manager's row menu
/// does.
mod release_storage_action {
    use super::*;

    fn summary(actions: Vec<AutomationReleaseStorageAction>) -> AutomationReleaseSummary {
        AutomationReleaseSummary {
            id: "release-1".to_string(),
            album_id: "album-1".to_string(),
            media: Vec::new(),
            storage_state: AutomationReleaseStorageState::InStorage { pinned: false },
            storage_actions: actions,
            transfer_action: None,
            upload: None,
            transitioning: false,
            file_count: 11,
            total_size: 320_000_000,
            cover: None,
        }
    }

    fn parse(args: Value) -> ReleaseStorageActionInput {
        from_value::<ReleaseStorageActionInput>(args).expect("the tool's own input shape")
    }

    /// Every action arrives under the name the bridge gives it, with what it
    /// needs.
    #[test]
    fn each_action_parses_from_its_wire_shape() {
        let moved = parse(serde_json::json!({
            "release_id": "release-1",
            "action": { "kind": "upload" },
        }));
        assert_eq!(moved.release_id, "release-1");
        assert!(matches!(moved.action, AutomationStorageAction::Upload));

        assert!(matches!(
            parse(serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": "detach_audio", "destination_dir": "/music/out" },
            }))
            .action,
            AutomationStorageAction::DetachAudio { destination_dir } if destination_dir == "/music/out"
        ));

        for (kind, expected) in [
            ("pin", AutomationStorageAction::Pin),
            ("unpin", AutomationStorageAction::Unpin),
            ("cancel", AutomationStorageAction::Cancel),
        ] {
            let parsed = parse(serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": kind },
            }));
            assert_eq!(
                std::mem::discriminant(&parsed.action),
                std::mem::discriminant(&expected),
                "'{kind}' names its action"
            );
        }
    }

    /// The names a release reports among its transitions, and lists when one
    /// is refused, are the names the tool accepts.
    #[test]
    fn reported_transition_names_are_the_accepted_names() {
        use AutomationReleaseStorageAction as A;
        for action in [A::Upload, A::Pin, A::Unpin, A::DetachAudio] {
            let reported = serde_json::to_value(action).unwrap();
            let name = reported.as_str().expect("a transition reports as a name");
            parse(serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": name, "destination_dir": "/music/out" },
            }));
            let refusal = require_action(&summary(vec![action]), A::Unpin, "unpin")
                .err()
                .filter(|_| action != A::Unpin);
            if let Some(refusal) = refusal {
                assert!(
                    refusal.message().ends_with(&format!("available: {name}")),
                    "the refusal lists {name} as reported: {}",
                    refusal.message()
                );
            }
        }
    }

    /// Detaching audio needs a folder; there is no default for it.
    #[test]
    fn an_action_missing_what_it_needs_is_refused() {
        let error = from_value::<ReleaseStorageActionInput>(serde_json::json!({
            "release_id": "release-1",
            "action": { "kind": "detach_audio" },
        }))
        .expect_err("an action without its required field is not an action");
        assert_eq!(error.kind(), "validation");
    }

    /// A transition core doesn't offer is refused before any transfer starts.
    #[test]
    fn only_the_transitions_core_offers_are_run() {
        let pinnable = summary(vec![
            AutomationReleaseStorageAction::Pin,
            AutomationReleaseStorageAction::DetachAudio,
        ]);

        require_action(&pinnable, AutomationReleaseStorageAction::Pin, "pin")
            .expect("core offers the pin");

        let error = require_action(&pinnable, AutomationReleaseStorageAction::Unpin, "unpin")
            .expect_err("core does not offer the unpin");
        assert_eq!(error.kind(), "validation");
        assert!(
            error.message().contains("pin, detach_audio"),
            "the refusal names what the release does offer: {}",
            error.message()
        );
    }

    /// A library with no cloud home says so rather than listing no transitions.
    #[test]
    fn a_release_with_no_transitions_says_why() {
        let error =
            require_action(&summary(Vec::new()), AutomationReleaseStorageAction::Upload, "upload")
                .expect_err("a library with no cloud home cannot move anything to it");
        assert!(
            error.message().contains("no cloud home"),
            "unexpected refusal: {}",
            error.message()
        );
    }

    /// An upload reports the release whose uploads it queued.
    #[test]
    fn the_outcome_carries_what_the_transition_produced() {
        let json = serde_json::to_value(AutomationStorageActionOutcome::UploadQueued {
            release_id: "release-1".to_string(),
        })
        .unwrap();
        assert_eq!(json["kind"], "upload_queued");
        assert_eq!(json["release_id"], "release-1");

        let json = serde_json::to_value(AutomationStorageActionOutcome::PinQueued {
            release_id: "release-1".to_string(),
        })
        .unwrap();
        assert_eq!(json["kind"], "pin_queued");
    }

    /// The tool is reachable by the name its schema is published under.
    #[test]
    fn the_tool_dispatches_by_name() {
        assert_eq!(
            AutomationTool::from_name("release_storage_action"),
            Some(AutomationTool::ReleaseStorageAction)
        );
        assert!(!AutomationTool::ReleaseStorageAction.accepts_missing_arguments());
    }
}
