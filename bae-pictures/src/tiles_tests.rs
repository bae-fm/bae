use super::*;

#[test]
fn levels_halve_from_the_picture_s_size_down_to_one_tile() {
    let pyramid = Pyramid { width: 6000, height: 5600, format: CopyFormat::Jpeg };
    let sizes: Vec<_> = pyramid.levels().iter().map(|level| (level.width, level.height)).collect();
    assert_eq!(sizes, [(6000, 5600), (3000, 2800), (1500, 1400), (750, 700), (375, 350)]);
    let first = pyramid.levels()[0];
    assert_eq!((first.columns, first.rows), (12, 11));
}

#[test]
fn a_picture_that_fits_one_tile_has_one_level() {
    let pyramid = Pyramid { width: 512, height: 300, format: CopyFormat::Png };
    assert_eq!(pyramid.levels(), [TileLevel { width: 512, height: 300, columns: 1, rows: 1 }]);
}

#[test]
fn a_tile_s_file_is_in_its_level_s_folder() {
    let folder = Path::new("/tiles");
    assert_eq!(tile_path(folder, CopyFormat::Jpeg, 2, 3, 4), Path::new("/tiles/2/3_4.jpg"));
}
