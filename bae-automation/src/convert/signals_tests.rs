#![cfg(desktop)]

use super::*;
use bae_identify::signals::{BarcodeSignal, DiscIdSignal, Signals, SourcedValue, TextSignal};

#[test]
fn signals_map_all_three_subsignals() {
    let signals = Signals {
        origin: bae_catalogs::AudioOrigin {
            source: Some(bae_catalogs::AudioSource::CdRip {
                proof: bae_catalogs::CdProof::RipLog,
                file: Some("Album.log".to_string()),
            }),
            not_cd_rate: None,
        },
        disc_id: DiscIdSignal::Computed {
            discs: vec![bae_identify::signals::ComputedDiscId {
                disc_id: "disc-hash".to_string(),
                source_file: None,
            }],
        },
        barcode: BarcodeSignal::Settled {
            codes: vec![SourcedValue::new("0123456789012".to_string())],
        },
        text: TextSignal::Settled {
            catalogs: vec!["CAT-1".to_string()],
            free_text: vec!["Album Title".to_string()],
        },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    };

    let json = serde_json::to_value(AutomationSignals::from_core(signals)).unwrap();
    assert_eq!(json["origin"]["source"]["kind"], "cd_rip");
    assert_eq!(json["origin"]["source"]["proof"], "rip_log");
    assert_eq!(json["origin"]["source"]["file"], "Album.log");
    assert_eq!(json["origin"]["not_cd_rate"], serde_json::Value::Null);
    assert_eq!(json["disc_id"]["kind"], "computed");
    assert_eq!(json["disc_id"]["disc_ids"], serde_json::json!(["disc-hash"]));
    assert_eq!(json["barcode"]["kind"], "settled");
    assert_eq!(json["barcode"]["codes"][0], "0123456789012");
    assert_eq!(json["text"]["kind"], "settled");
    assert_eq!(json["text"]["catalogs"][0], "CAT-1");
    assert_eq!(json["text"]["free_text"][0], "Album Title");
}
