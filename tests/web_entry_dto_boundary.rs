#![cfg(feature = "web-entry")]

use unredact::service::unredact_web_entry::{
    UnredactWebConfig, UnredactWebOutputs, UnredactWebRequest,
};
use unredact::types::guess_types::GuessConfig;
use unredact::types::visualizer_config::VisualizerConfig;

#[test]
fn web_request_dto_roundtrips_via_json() {
    let request = UnredactWebRequest {
        input_name: "sample.pdf".to_owned(),
        pdf_bytes: vec![1_u8, 2_u8, 3_u8, 4_u8],
        dictionary_file_bytes: Some(b"SARAH KELLEN\nNADIA MARCINKOVA\n".to_vec()),
        cfg: UnredactWebConfig {
            include_details: true,
            enable_image_analysis: false,
            raster_dpi: 96.0_f32,
            guess: GuessConfig {
                visual_score: true,
                visual_score_dpi: 200.0_f32,
            },
            visualize: true,
            visualizer: VisualizerConfig::default(),
        },
    };

    let encoded = serde_json::to_vec(&request)
        .unwrap_or_else(|error| panic!("failed to encode request DTO: {error}"));
    let decoded: UnredactWebRequest = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("failed to decode request DTO: {error}"));

    assert_eq!(decoded, request);
}

#[test]
fn web_output_dto_roundtrips_via_json() {
    let output = UnredactWebOutputs {
        redactions_json: b"{\"count\":1}".to_vec(),
        fonts_json: b"{\"inputs\":[]}".to_vec(),
        guesses_json: b"{\"guesses\":[]}".to_vec(),
        visualized_pdf_bytes: Some(vec![9_u8, 8_u8, 7_u8]),
    };

    let encoded = serde_json::to_vec(&output)
        .unwrap_or_else(|error| panic!("failed to encode output DTO: {error}"));
    let decoded: UnredactWebOutputs = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("failed to decode output DTO: {error}"));

    assert_eq!(decoded, output);
}
