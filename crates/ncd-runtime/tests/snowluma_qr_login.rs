use ncd_domain::{SnowlumaQrFailureCategory, SnowlumaQrLoginStatus};
use ncd_runtime::remote::snowluma_qr_login::{
    calibrated_click_points, validate_display, QrDecoder, QrLoginPoint, QrLoginWindowGeometry,
    UnavailableQrDecoder,
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
    assert_eq!(SnowlumaQrLoginStatus::FallbackNoVnc as u8, 3);
}
