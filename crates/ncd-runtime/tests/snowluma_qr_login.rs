use ncd_domain::SnowlumaQrFailureCategory;
use ncd_runtime::remote::snowluma_qr_login::{
    QrDecoder, QrLoginPoint, QrLoginWindowGeometry, UnavailableQrDecoder, calibrated_click_points,
    validate_display,
};

#[test]
fn current_variant_recalculates_clicks_from_window_geometry() {
    let geometry = QrLoginWindowGeometry {
        x: 480,
        y: 130,
        width: 320,
        height: 460,
    };
    assert_eq!(
        calibrated_click_points(geometry).unwrap(),
        vec![
            QrLoginPoint { x: 640, y: 410 },
            QrLoginPoint { x: 600, y: 554 },
            QrLoginPoint { x: 600, y: 552 },
        ]
    );
}

#[test]
fn display_validation_rejects_command_injection() {
    assert!(validate_display(":0").is_ok());
    assert!(validate_display(":0; touch /tmp/pwned").is_err());
}

#[test]
fn missing_decoder_is_an_explicit_novnc_fallback() {
    let error = UnavailableQrDecoder.decode(&[]).unwrap_err();
    assert_eq!(error, SnowlumaQrFailureCategory::DecoderUnavailable);
}

#[test]
fn serialized_fallback_keeps_the_reason_visible() {
    let session = ncd_domain::SnowlumaQrLoginSession {
        server_id: "srv".into(),
        bot_id: "10001".into(),
        session_id: ncd_domain::QrLoginSessionId::new("session").unwrap(),
        capture_generation: 0,
    };
    let result = ncd_domain::SnowlumaQrLoginResult::FallbackNoVnc {
        session,
        reason: SnowlumaQrFailureCategory::DecoderUnavailable,
    };
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["status"], "fallback_no_vnc");
    assert_eq!(json["reason"], "decoder_unavailable");
}
