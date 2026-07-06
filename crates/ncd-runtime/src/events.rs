// 事件总线 re-export 层
//
// EventBus trait / BroadcastEventBus / EventFilter / EventSubscription 已下沉到
// ncd-traits::events（Layer 2），DomainEvent 等数据类型在 ncd-domain（Layer 1）。
// 本模块仅做 re-export，保持下游 use ncd_runtime::events::* 不中断。

// re-export 向后兼容: EventBus trait + BroadcastEventBus + EventFilter + EventSubscription
pub use ncd_traits::events::{
    BroadcastEventBus, DEFAULT_BROADCAST_CAPACITY, EventBus, EventFilter, EventSubscription,
};

// re-export 向后兼容: DomainEvent 数据类型
pub use ncd_domain::domain_event::{DOMAIN_EVENT_ENVELOPE_VERSION, DomainEvent, DomainEventKind};
pub use ncd_domain::napcat_events::NapCatLoginInvalidationReason;

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::bot_actor::BotActorSnapshot;
    use ncd_domain::bot_status::BotStatus;
    use ncd_domain::daemon_state::{DaemonState, SnowLumaLoginState};
    use ncd_domain::deployment_task::{
        DeploymentTaskKind, DeploymentTaskSnapshot, DeploymentTaskStatus,
    };
    use ncd_domain::progress::ProgressEvent as NcdProgressEvent;
    use ncd_domain::progress::ProgressKind as NcdProgressKind;

    #[test]
    fn event_name_mapping_matches_frontend_contract() {
        let event = DomainEvent::bot_log("10001", "hello");
        assert_eq!(event.tauri_event_name(), "bot_log_appended");
        assert_eq!(event.kind(), DomainEventKind::BotLogAppended);
    }

    #[tokio::test]
    async fn broadcast_event_bus_filters_by_bot_id() {
        let bus = BroadcastEventBus::default();
        let mut subscription = bus.subscribe(EventFilter::bot("10001"));

        bus.publish(DomainEvent::bot_log("10002", "skip"));
        bus.publish(DomainEvent::bot_log("10001", "hit"));

        let event = subscription.next().await.expect("expected matching event");
        match event {
            DomainEvent::BotLogAppended { bot_id, line, .. } => {
                assert_eq!(bot_id.as_str(), "10001");
                assert_eq!(line, "hit");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn bot_status_changed_event_serializes() {
        let status = BotStatus::running("10004", 1234, 5678);
        let event = DomainEvent::bot_status_changed(status, "runtime_poll");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("bot_status_changed"));
        assert!(json.contains("runtime_poll"));
    }

    #[test]
    fn bot_state_changed_event_serializes() {
        let snapshot = BotActorSnapshot::new("10003");
        let event = DomainEvent::bot_state_changed(snapshot, "start_requested");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("bot_state_changed"));
        assert!(json.contains("start_requested"));
    }

    const FRONTEND_EVENTS_TS: &str =
        include_str!("../../../src-ui/core/services/event-stream.service.ts");

    fn assert_round_trip(event: DomainEvent) {
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: DomainEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, event, "round-trip must preserve equality");
        let json2 = serde_json::to_string(&decoded).expect("re-serialize");
        assert_eq!(json, json2, "byte-level round-trip must be stable");
    }

    #[test]
    fn napcat_login_qrcode_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_qrcode(
            "10001",
            "data:image/png;base64,AAAA",
        ));
    }

    #[test]
    fn napcat_login_qrcode_removed_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_qrcode_removed("10001"));
    }

    #[test]
    fn napcat_login_online_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_online("10001", true));
        assert_round_trip(DomainEvent::napcat_login_online("10001", false));
    }

    #[test]
    fn napcat_login_invalidated_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_invalidated(
            "10001",
            NapCatLoginInvalidationReason::Kicked,
        ));
        assert_round_trip(DomainEvent::napcat_login_invalidated(
            "10001",
            NapCatLoginInvalidationReason::LoggedOut,
        ));
    }

    #[test]
    fn napcat_login_invalidation_reason_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&NapCatLoginInvalidationReason::Kicked).unwrap(),
            "\"kicked\""
        );
        assert_eq!(
            serde_json::to_string(&NapCatLoginInvalidationReason::LoggedOut).unwrap(),
            "\"logged_out\""
        );
    }

    #[test]
    fn napcat_login_event_name_literals_are_stable() {
        let cases: [(DomainEvent, &str); 4] = [
            (
                DomainEvent::napcat_login_qrcode("10001", "url"),
                "napcat_login_qrcode",
            ),
            (
                DomainEvent::napcat_login_qrcode_removed("10001"),
                "napcat_login_qrcode_removed",
            ),
            (
                DomainEvent::napcat_login_online("10001", true),
                "napcat_login_online",
            ),
            (
                DomainEvent::napcat_login_invalidated(
                    "10001",
                    NapCatLoginInvalidationReason::Kicked,
                ),
                "napcat_login_invalidated",
            ),
        ];
        for (event, expected) in &cases {
            assert_eq!(event.tauri_event_name(), *expected);
        }
    }

    #[test]
    fn napcat_login_event_names_are_present_in_frontend_events_ts() {
        let names = [
            "napcat_login_qrcode",
            "napcat_login_qrcode_removed",
            "napcat_login_online",
            "napcat_login_invalidated",
        ];
        for name in &names {
            let needle_single = format!("'{name}'");
            let needle_double = format!("\"{name}\"");
            assert!(
                FRONTEND_EVENTS_TS.contains(&needle_single)
                    || FRONTEND_EVENTS_TS.contains(&needle_double),
                "frontend event-stream.service.ts must contain literal {name:?} \
as a quoted string",
            );
        }
    }

    #[test]
    fn every_domain_event_variant_is_listed_in_frontend_events_ts() {
        let snapshot = BotActorSnapshot::new("10001");
        let status = BotStatus::running("10001", 1, 0);
        let all: Vec<DomainEvent> = vec![
            DomainEvent::bot_state_changed(snapshot, "init"),
            DomainEvent::bot_status_changed(status, "init"),
            DomainEvent::bot_log("10001", "x"),
            DomainEvent::bot_error("10001", "x", None),
            DomainEvent::task_progress("t", 0, "x"),
            DomainEvent::napcat_webui_available("10001", 6099, "tk"),
            DomainEvent::bot_process_exited("10001", Some(0), None),
            DomainEvent::napcat_login_qrcode("10001", "url"),
            DomainEvent::napcat_login_qrcode_removed("10001"),
            DomainEvent::napcat_login_online("10001", true),
            DomainEvent::napcat_login_invalidated("10001", NapCatLoginInvalidationReason::Kicked),
            DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                1,
                None,
                Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            ),
            DomainEvent::snowluma_bot_injected("10001", 12345),
            DomainEvent::snowluma_uin_detected("10001", "100200"),
            DomainEvent::snowluma_login_state_changed("10001", SnowLumaLoginState::LoggedIn),
            DomainEvent::snowluma_pid_set_changed("10001", vec![1234, 5678]),
            DomainEvent::snowluma_daemon_log("hello world"),
            DomainEvent::component_action_progress(
                "task-1",
                NcdProgressEvent::new(NcdProgressKind::Started { total_steps: 3 }),
            ),
            DomainEvent::docker_deploy_progress(
                "task-2",
                NcdProgressEvent::new(NcdProgressKind::Started { total_steps: 5 }),
            ),
            DomainEvent::docker_install_progress(
                "task-3",
                NcdProgressEvent::new(NcdProgressKind::Started { total_steps: 7 }),
            ),
            DomainEvent::deployment_task_changed(DeploymentTaskSnapshot {
                task_id: "task-4".to_string(),
                kind: DeploymentTaskKind::DockerInstall,
                status: DeploymentTaskStatus::Queued,
                host_id: "remote:a".to_string(),
                title: "Docker 安装".to_string(),
                dedupe_key: None,
                resources: vec![],
                progress_events: vec![],
                submitted_at_ms: 1,
                started_at_ms: None,
                ended_at_ms: None,
                message: None,
                error: None,
                cancellable: false,
            }),
            DomainEvent::desktop_log_appended("desktop line"),
        ];
        for event in &all {
            let name = event.tauri_event_name();
            let needle_single = format!("'{name}'");
            let needle_double = format!("\"{name}\"");
            assert!(
                FRONTEND_EVENTS_TS.contains(&needle_single)
                    || FRONTEND_EVENTS_TS.contains(&needle_double),
                "DomainEvent::{:?} tauri_event_name {name:?} not in frontend DOMAIN_EVENT_NAMES",
                event.kind(),
            );
        }
    }

    #[test]
    fn snowluma_daemon_state_changed_round_trips() {
        assert_round_trip(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Ready,
            1,
            None,
            Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
        ));
        assert_round_trip(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Crashed,
            0,
            Some("node child exited unexpectedly".into()),
            Some("srv-test".into()),
        ));
    }

    #[test]
    fn snowluma_bot_injected_round_trips() {
        assert_round_trip(DomainEvent::snowluma_bot_injected("10001", 12345));
    }

    #[test]
    fn snowluma_uin_detected_round_trips() {
        assert_round_trip(DomainEvent::snowluma_uin_detected("10001", "100200"));
    }

    #[test]
    fn snowluma_login_state_changed_round_trips() {
        assert_round_trip(DomainEvent::snowluma_login_state_changed(
            "10001",
            SnowLumaLoginState::LoggedIn,
        ));
        assert_round_trip(DomainEvent::snowluma_login_state_changed(
            "10001",
            SnowLumaLoginState::WaitingForQrScan,
        ));
    }

    #[test]
    fn snowluma_pid_set_changed_round_trips() {
        assert_round_trip(DomainEvent::snowluma_pid_set_changed(
            "10001",
            vec![1234, 5678],
        ));
        assert_round_trip(DomainEvent::snowluma_pid_set_changed("10001", vec![]));
    }

    #[test]
    fn snowluma_daemon_log_round_trips() {
        assert_round_trip(DomainEvent::snowluma_daemon_log("hello world"));
    }

    #[test]
    fn snowluma_event_name_literals_are_stable() {
        let cases: [(DomainEvent, &str); 6] = [
            (
                DomainEvent::snowluma_daemon_state_changed(
                    DaemonState::Ready,
                    1,
                    None,
                    Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
                ),
                "snowluma_daemon_state_changed",
            ),
            (
                DomainEvent::snowluma_bot_injected("10001", 12345),
                "snowluma_bot_injected",
            ),
            (
                DomainEvent::snowluma_uin_detected("10001", "100200"),
                "snowluma_uin_detected",
            ),
            (
                DomainEvent::snowluma_login_state_changed("10001", SnowLumaLoginState::LoggedIn),
                "snowluma_login_state_changed",
            ),
            (
                DomainEvent::snowluma_pid_set_changed("10001", vec![1234, 5678]),
                "snowluma_pid_set_changed",
            ),
            (
                DomainEvent::snowluma_daemon_log("hello world"),
                "snowluma_daemon_log",
            ),
        ];
        for (event, expected) in &cases {
            assert_eq!(event.tauri_event_name(), *expected);
        }
    }

    #[test]
    fn snowluma_event_names_are_present_in_frontend_events_ts() {
        let names = [
            "snowluma_daemon_state_changed",
            "snowluma_bot_injected",
            "snowluma_uin_detected",
            "snowluma_login_state_changed",
            "snowluma_pid_set_changed",
            "snowluma_daemon_log",
        ];
        for name in &names {
            let needle_single = format!("'{name}'");
            let needle_double = format!("\"{name}\"");
            assert!(
                FRONTEND_EVENTS_TS.contains(&needle_single)
                    || FRONTEND_EVENTS_TS.contains(&needle_double),
                "frontend event-stream.service.ts must contain literal {name:?}",
            );
        }
    }

    #[test]
    fn component_action_progress_round_trips() {
        let evt = NcdProgressEvent::new(NcdProgressKind::StepBegin {
            step: 2,
            message: "downloading".to_string(),
        });
        assert_round_trip(DomainEvent::component_action_progress("task-1", evt));
    }

    #[test]
    fn component_action_progress_event_name_literal_is_stable() {
        let evt = NcdProgressEvent::new(NcdProgressKind::Started { total_steps: 1 });
        let event = DomainEvent::component_action_progress("task-1", evt);
        assert_eq!(event.tauri_event_name(), "component_action_progress");
        assert_eq!(event.kind(), DomainEventKind::ComponentActionProgress);
        assert_eq!(event.bot_id(), None);
    }

    #[test]
    fn component_action_progress_event_name_present_in_frontend_events_ts() {
        let name = "component_action_progress";
        let needle_single = format!("'{name}'");
        let needle_double = format!("\"{name}\"");
        assert!(
            FRONTEND_EVENTS_TS.contains(&needle_single)
                || FRONTEND_EVENTS_TS.contains(&needle_double),
        );
    }

    #[test]
    fn docker_deploy_progress_round_trips() {
        let evt = NcdProgressEvent::new(NcdProgressKind::StepProgress {
            step: 3,
            percent: 68,
            message: "pulling napcat-docker".to_string(),
            speed_bps: Some(2_400_000),
            downloaded_bytes: Some(327_000_000),
            total_bytes: Some(480_000_000),
            download_stage: Some("streaming".to_string()),
            docker_layers: None,
        });
        assert_round_trip(DomainEvent::docker_deploy_progress("task-2", evt));
    }

    #[test]
    fn docker_deploy_progress_event_name_literal_is_stable() {
        let evt = NcdProgressEvent::new(NcdProgressKind::Started { total_steps: 5 });
        let event = DomainEvent::docker_deploy_progress("task-2", evt);
        assert_eq!(event.tauri_event_name(), "docker_deploy_progress");
        assert_eq!(event.kind(), DomainEventKind::DockerDeployProgress);
        assert_eq!(event.bot_id(), None);
    }

    #[test]
    fn docker_deploy_progress_event_name_present_in_frontend_events_ts() {
        let name = "docker_deploy_progress";
        let needle_single = format!("'{name}'");
        let needle_double = format!("\"{name}\"");
        assert!(
            FRONTEND_EVENTS_TS.contains(&needle_single)
                || FRONTEND_EVENTS_TS.contains(&needle_double),
        );
    }

    #[test]
    fn domain_event_envelope_carries_version_and_preserves_payload() {
        let json = DomainEvent::bot_log("10001", "hello")
            .to_envelope_json()
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["v"], DOMAIN_EVENT_ENVELOPE_VERSION);
        assert_eq!(value["kind"], "bot_log_appended");
        assert_eq!(value["bot_id"], "10001");
        assert_eq!(value["line"], "hello");

        let status = DomainEvent::bot_status_changed(BotStatus::running("10001", 1, 2), "poll");
        let sv: serde_json::Value =
            serde_json::from_str(&status.to_envelope_json().unwrap()).unwrap();
        assert_eq!(sv["v"], DOMAIN_EVENT_ENVELOPE_VERSION);
        assert_eq!(sv["kind"], "bot_status_changed");
        assert!(sv["status"].is_object());
    }

    #[test]
    fn key_event_payloads_lock_wire_field_names() {
        fn sorted_keys(event: &DomainEvent) -> Vec<String> {
            let value = serde_json::to_value(event).unwrap();
            let mut keys: Vec<String> = value
                .as_object()
                .expect("DomainEvent must serialize as object")
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        }

        assert_eq!(
            sorted_keys(&DomainEvent::bot_error("1", "m", Some("h".into()))),
            vec!["bot_id", "hint", "kind", "message"]
        );
        assert_eq!(
            sorted_keys(&DomainEvent::bot_log("1", "l")),
            vec!["bot_id", "kind", "line"]
        );
        assert_eq!(
            sorted_keys(&DomainEvent::napcat_webui_available("1", 6099, "t")),
            vec!["bot_id", "kind", "port", "token"]
        );
        assert_eq!(
            sorted_keys(&DomainEvent::bot_process_exited(
                "1",
                Some(0),
                Some("r".into())
            )),
            vec!["bot_id", "exit_code", "kind", "reason"]
        );
        assert_eq!(
            sorted_keys(&DomainEvent::napcat_login_invalidated(
                "1",
                NapCatLoginInvalidationReason::Kicked
            )),
            vec!["bot_id", "kind", "reason"]
        );
    }
}
