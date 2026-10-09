use super::*;

#[test]
fn fusionauth_kickstart_is_privately_relabelled_and_replaces_inherited_mounts() {
    for name in [None, Some("selinux-test")] {
        let instance = Instance::derive(name, None).unwrap();
        let mut services = IndexMap::new();
        add_local_infra(&mut services, &instance, false);
        let compose = dct::Compose {
            services: dct::Services(services),
            ..Default::default()
        };
        let mut value = serde_yaml::to_value(compose).unwrap();
        apply_tags(&mut value, Mode::Local, &instance);

        let Value::Tagged(volumes) = &value["services"]["fusionauth"]["volumes"] else {
            panic!("FusionAuth mounts must replace the inherited mount list");
        };
        assert_eq!(volumes.tag, Tag::new("!override"));
        assert_eq!(
            volumes.value,
            serde_yaml::to_value(vec![
                "fusionauth_config:/usr/local/fusionauth/config".to_string(),
                format!(
                    "{}:/usr/local/fusionauth/kickstart:ro,Z",
                    kickstart_dir(&instance).display()
                ),
            ])
            .unwrap()
        );
    }
}

#[test]
fn exposure_hardening_binds_ports_to_loopback_and_drops_risky_mounts() {
    let mut value: Value = serde_yaml::from_str(
        r#"
services:
  postgres:
    ports: ["31000:5432"]
  proxy:
    ports: ["31009:31009", "31032:8443"]
  mailpit:
    ports: ["31007:1025", "31008:8025"]
  agent_harness_service:
    ports: ["31026:8080"]
  sdk-webhook-relay:
    ports: ["127.0.0.1:31023:22"]
"#,
    )
    .unwrap();
    // Exercise the tagged (`!override`) form too.
    override_in_place(
        value["services"]["postgres"].as_mapping_mut().unwrap(),
        "ports",
    );
    harden_for_exposure(&mut value, &["/bin:/app/out:ro".to_string()]);
    let yaml = serde_yaml::to_string(&value).unwrap();
    assert!(yaml.contains("127.0.0.1:31000:5432"), "{yaml}");
    assert!(yaml.contains("127.0.0.1:31009:31009"), "{yaml}");
    assert!(yaml.contains("127.0.0.1:31026:8080"), "{yaml}");
    assert!(yaml.contains("127.0.0.1:31023:22"), "{yaml}");
    assert!(
        !yaml.contains("31007"),
        "mailpit must publish nothing: {yaml}"
    );
    assert!(!yaml.contains("docker.sock"), "{yaml}");
    assert!(yaml.contains("/bin:/app/out:ro"), "{yaml}");
    assert!(
        !yaml.contains("- 31"),
        "every mapping is loopback-bound: {yaml}"
    );
}
