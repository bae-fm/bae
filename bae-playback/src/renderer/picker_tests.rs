//! `cast_device`: each write lists the whole picker, read by name, and
//! opening the library empties it.

use super::*;
use bae_store::test_fixtures::temp_db;

fn device(id: &str, name: &str, kind: RendererKind) -> CastDevice {
    CastDevice { id: id.to_string(), name: name.to_string(), kind }
}

/// A write replaces the whole list; the list reads by name, then id.
#[tokio::test]
async fn each_write_lists_the_whole_picker_by_name() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db);
    picker
        .show(vec![
            device("uuid:tv", "Television", RendererKind::Dlna),
            device("cast-1", "Kitchen", RendererKind::Cast),
            device("airplay-1", "Kitchen", RendererKind::AirPlay),
        ])
        .await
        .unwrap();
    assert_eq!(
        picker.devices().await.unwrap(),
        vec![
            device("airplay-1", "Kitchen", RendererKind::AirPlay),
            device("cast-1", "Kitchen", RendererKind::Cast),
            device("uuid:tv", "Television", RendererKind::Dlna),
        ]
    );

    picker.show(vec![device("cast-1", "Kitchen", RendererKind::Cast)]).await.unwrap();
    assert_eq!(
        picker.devices().await.unwrap(),
        vec![device("cast-1", "Kitchen", RendererKind::Cast)]
    );
}

/// Opening the library empties the list: no discovery outlives the process
/// that ran it.
#[tokio::test]
async fn opening_empties_the_picker() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let picker = CastPicker::new(db);
    picker.show(vec![device("cast-1", "Kitchen", RendererKind::Cast)]).await.unwrap();

    picker.clear().await.unwrap();

    assert_eq!(picker.devices().await.unwrap(), Vec::new());
}
