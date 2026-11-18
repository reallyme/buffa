// Modified by ReallyMe Technologies Limited.
use super::*;

fn descriptor() -> FileDescriptorProto {
    let mut file = proto3_file("cleanup.proto");
    file.package = Some("cleanup".into());
    file.message_type.push(DescriptorProto {
        name: Some("Record".into()),
        field: vec![
            make_field("handle", 1, Label::LABEL_OPTIONAL, Type::TYPE_UINT64),
            make_field("values", 2, Label::LABEL_REPEATED, Type::TYPE_FLOAT),
        ],
        ..Default::default()
    });
    file
}

#[test]
fn cleanup_is_opt_in_and_selects_guarded_collections() {
    let file = descriptor();
    let mut config = CodeGenConfig::default();
    let plain = joined(
        &generate(
            core::slice::from_ref(&file),
            &["cleanup.proto".into()],
            &config,
        )
        .unwrap(),
    );
    assert!(!plain.contains("impl ::core::ops::Drop"));
    config.preserve_unknown_fields = false;
    config.zeroize_messages = vec![".cleanup".into()];
    config.generate_json = true;
    let guarded = joined(&generate(&[file], &["cleanup.proto".into()], &config).unwrap());
    assert!(guarded.contains("impl ::core::ops::Drop for Record"));
    assert!(guarded.contains("::buffa::sensitive::List<f32>"));
    assert!(guarded.contains("__requires_wipe_owner"));
    assert!(!guarded.contains("..Default::default()"));
}

#[test]
fn cleanup_rejects_unknown_storage_plain_buffers_and_unmatched_rules() {
    let mut config = CodeGenConfig {
        generate_zeroize: true,
        ..Default::default()
    };
    assert!(matches!(
        generate(&[descriptor()], &["cleanup.proto".into()], &config),
        Err(CodeGenError::InvalidZeroizeConfiguration {
            reason: ZeroizeConfigurationErrorReason::UnknownFieldStorage,
            ..
        })
    ));
    config.preserve_unknown_fields = false;
    let mut file = descriptor();
    file.message_type[0].field.push(make_field(
        "secret",
        3,
        Label::LABEL_OPTIONAL,
        Type::TYPE_STRING,
    ));
    assert!(matches!(
        generate(
            core::slice::from_ref(&file),
            &["cleanup.proto".into()],
            &config
        ),
        Err(CodeGenError::InvalidZeroizeConfiguration {
            reason: ZeroizeConfigurationErrorReason::BufferRepresentation,
            ..
        })
    ));
    config.string_fields = vec![(
        ".cleanup".into(),
        StringRepr::Custom("crate::SecretString".into()),
    )];
    config.generate_json = true;
    assert!(matches!(
        generate(
            core::slice::from_ref(&file),
            &["cleanup.proto".into()],
            &config
        ),
        Err(CodeGenError::InvalidZeroizeConfiguration {
            reason: ZeroizeConfigurationErrorReason::JsonPolicy,
            ..
        })
    ));
    config.json_policy_types = vec!["crate::SecretString".into()];
    assert!(generate(&[file], &["cleanup.proto".into()], &config).is_ok());
    config.generate_zeroize = false;
    config.json_policy_types.clear();
    config.zeroize_messages = vec![".cleanup.Misspelled".into()];
    assert!(matches!(
        generate(&[descriptor()], &["cleanup.proto".into()], &config),
        Err(CodeGenError::ZeroizeRuleMatchedNothing { .. })
    ));
}

#[test]
fn cleanup_arbitrary_constructs_an_owner_and_bounds_recursion() {
    let config = CodeGenConfig {
        generate_zeroize: true,
        preserve_unknown_fields: false,
        generate_arbitrary: true,
        ..Default::default()
    };
    let code = joined(&generate(&[descriptor()], &["cleanup.proto".into()], &config).unwrap());
    assert!(code.contains("GuardedArbitrary<'a> for Record"));
    assert!(code.contains("__remaining_depth"));
    assert!(code.contains("checked_sub(1)"));
    assert!(code.contains("let mut __owned = Self::default()"));
    assert!(!code.contains("derive(::arbitrary::Arbitrary)"));
}

#[test]
fn directory_scoping_is_explicit_and_retains_selector_diagnostics() {
    let mut config = CodeGenConfig {
        preserve_unknown_fields: false,
        zeroize_messages: vec![".cleanup".into(), ".another_directory".into()],
        deny_view_json: vec![".another_directory".into()],
        ..Default::default()
    };
    assert!(matches!(
        generate(&[descriptor()], &["cleanup.proto".into()], &config),
        Err(CodeGenError::ZeroizeRuleMatchedNothing { .. })
    ));
    config.allow_unmatched_message_rules = true;
    let (_, warnings) =
        generate_with_diagnostics(&[descriptor()], &["cleanup.proto".into()], &config).unwrap();
    assert!(warnings
        .iter()
        .any(|warning| matches!(warning, CodeGenWarning::ZeroizeRuleMatchedNothing { .. })));
    assert!(warnings
        .iter()
        .any(|warning| matches!(warning, CodeGenWarning::ViewJsonRuleMatchedNothing { .. })));
}

#[test]
fn ordinary_reflection_cannot_snapshot_a_guarded_descendant() {
    let mut file = descriptor();
    let mut child = make_field("record", 1, Label::LABEL_OPTIONAL, Type::TYPE_MESSAGE);
    child.type_name = Some(".cleanup.Record".into());
    file.message_type.push(DescriptorProto {
        name: Some("Container".into()),
        field: vec![child],
        ..Default::default()
    });
    let config = CodeGenConfig {
        preserve_unknown_fields: false,
        zeroize_messages: vec![".cleanup.Record".into()],
        generate_reflection: true,
        ..Default::default()
    };
    assert!(matches!(
        generate(&[file], &["cleanup.proto".into()], &config),
        Err(CodeGenError::InvalidZeroizeConfiguration {
            reason: ZeroizeConfigurationErrorReason::ReflectionParent,
            ..
        })
    ));
}

#[test]
fn view_export_denial_propagates_across_cycles_and_imported_messages() {
    let mut main = proto3_file("parent.proto");
    main.package = Some("parent".into());
    let mut a_child = make_field("b", 1, Label::LABEL_OPTIONAL, Type::TYPE_MESSAGE);
    a_child.type_name = Some(".parent.B".into());
    a_child.oneof_index = Some(0);
    let mut b_child = make_field("a", 1, Label::LABEL_OPTIONAL, Type::TYPE_MESSAGE);
    b_child.type_name = Some(".parent.A".into());
    let mut secret = make_field("secret", 2, Label::LABEL_OPTIONAL, Type::TYPE_MESSAGE);
    secret.type_name = Some(".foreign.Private".into());
    main.message_type = vec![
        DescriptorProto {
            name: Some("A".into()),
            field: vec![a_child],
            oneof_decl: vec![OneofDescriptorProto {
                name: Some("choice".into()),
                ..Default::default()
            }],
            ..Default::default()
        },
        DescriptorProto {
            name: Some("B".into()),
            field: vec![b_child, secret],
            ..Default::default()
        },
        DescriptorProto {
            name: Some("Public".into()),
            ..Default::default()
        },
    ];
    let mut foreign = proto3_file("foreign.proto");
    foreign.package = Some("foreign".into());
    foreign.message_type.push(DescriptorProto {
        name: Some("Private".into()),
        ..Default::default()
    });
    let files = [main, foreign];
    let mut config = CodeGenConfig {
        generate_json: true,
        deny_view_json: vec![".foreign.Private".into()],
        ..Default::default()
    };
    let ctx = crate::context::CodeGenContext::new(&files, &config, &[]);
    assert!(ctx.denies_view_json("parent.A"));
    assert!(ctx.denies_view_json(".parent.B"));
    assert!(!ctx.denies_view_json("parent.Public"));
    assert!(generate(&files, &["parent.proto".into()], &config).is_ok());
    config.deny_view_json = vec![".foreign.Typo".into()];
    assert!(matches!(
        generate(&files, &["parent.proto".into()], &config),
        Err(CodeGenError::ViewJsonRuleMatchedNothing { .. })
    ));
}

#[test]
fn security_selectors_reject_empty_paths_even_for_scoped_requests() {
    for view_rule in [false, true] {
        let mut config = CodeGenConfig {
            allow_unmatched_message_rules: true,
            ..Default::default()
        };
        if view_rule {
            config.deny_view_json.push(String::new());
        } else {
            config.zeroize_messages.push(String::new());
        }
        let result = generate(&[descriptor()], &["cleanup.proto".into()], &config);
        if view_rule {
            assert!(matches!(
                result,
                Err(CodeGenError::ViewJsonRuleMatchedNothing { .. })
            ));
        } else {
            assert!(matches!(
                result,
                Err(CodeGenError::ZeroizeRuleMatchedNothing { .. })
            ));
        }
    }
}

#[test]
fn inactive_view_export_restrictions_warn_instead_of_disappearing() {
    for (json, views) in [(false, true), (true, false), (false, false)] {
        let config = CodeGenConfig {
            generate_json: json,
            generate_views: views,
            deny_view_json: vec![".cleanup.Record".into()],
            ..Default::default()
        };
        let (_, warnings) =
            generate_with_diagnostics(&[descriptor()], &["cleanup.proto".into()], &config).unwrap();
        assert!(warnings
            .iter()
            .any(|warning| matches!(warning, CodeGenWarning::ViewJsonRulesRequireJsonAndViews)));
    }
}

#[test]
fn ordinary_collections_cannot_relocate_a_guarded_descendant() {
    // message Secret { uint64 pin = 1; }
    // message Envelope { repeated Secret items = 1; map<int64, Secret> by_id = 2; }
    let mut file = proto3_file("partial.proto");
    file.package = Some("p".into());
    file.message_type.push(DescriptorProto {
        name: Some("Secret".into()),
        field: vec![make_field(
            "pin",
            1,
            Label::LABEL_OPTIONAL,
            Type::TYPE_UINT64,
        )],
        ..Default::default()
    });
    let mut items = make_field("items", 1, Label::LABEL_REPEATED, Type::TYPE_MESSAGE);
    items.type_name = Some(".p.Secret".into());
    let mut by_id = make_field("by_id", 2, Label::LABEL_REPEATED, Type::TYPE_MESSAGE);
    by_id.type_name = Some(".p.Envelope.ByIdEntry".into());
    let mut value = make_field("value", 2, Label::LABEL_OPTIONAL, Type::TYPE_MESSAGE);
    value.type_name = Some(".p.Secret".into());
    file.message_type.push(DescriptorProto {
        name: Some("Envelope".into()),
        field: vec![items, by_id],
        nested_type: vec![DescriptorProto {
            name: Some("ByIdEntry".into()),
            field: vec![
                make_field("key", 1, Label::LABEL_OPTIONAL, Type::TYPE_INT64),
                value,
            ],
            options: (MessageOptions {
                map_entry: Some(true),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut config = CodeGenConfig {
        preserve_unknown_fields: false,
        zeroize_messages: vec![".p.Secret".into()],
        generate_reflection: false,
        ..Default::default()
    };
    let error = generate(
        core::slice::from_ref(&file),
        &["partial.proto".into()],
        &config,
    )
    .unwrap_err();
    assert!(
        matches!(
            &error,
            CodeGenError::InvalidZeroizeConfiguration {
                message,
                reason: ZeroizeConfigurationErrorReason::ContainingMessage,
            } if message.as_str() == "p.Envelope"
        ),
        "{error:?}"
    );

    // Selecting the container too gives it wipe-owning collections.
    config.zeroize_messages.push(".p.Envelope".into());
    let generated = joined(
        &generate(
            core::slice::from_ref(&file),
            &["partial.proto".into()],
            &config,
        )
        .unwrap(),
    );
    assert!(generated.contains("::buffa::sensitive::List<Secret>"));
    assert!(generated.contains("::buffa::sensitive::Map<i64, Secret>"));

    // An explicit rule cannot put a plain standard collection back: it would
    // relocate the guarded elements just the same.
    for (repeated, map) in [
        (Some(RepeatedRepr::Vec), None),
        (
            Some(RepeatedRepr::Custom("::buffa::alloc::vec::Vec<*>".into())),
            None,
        ),
        (Some(RepeatedRepr::Custom("Vec<*>".into())), None),
        (None, Some(MapRepr::HashMap)),
        (None, Some(MapRepr::BTreeMap)),
    ] {
        let mut config = config.clone();
        config.repeated_fields = repeated
            .into_iter()
            .map(|repr| (".p.Envelope.items".to_owned(), repr))
            .collect();
        config.map_fields = map
            .into_iter()
            .map(|repr| (".p.Envelope.by_id".to_owned(), repr))
            .collect();
        let error = generate(
            core::slice::from_ref(&file),
            &["partial.proto".into()],
            &config,
        )
        .unwrap_err();
        assert!(
            matches!(
                &error,
                CodeGenError::InvalidZeroizeConfiguration {
                    reason: ZeroizeConfigurationErrorReason::CollectionRepresentation,
                    ..
                }
            ),
            "{error:?}"
        );
    }
}
